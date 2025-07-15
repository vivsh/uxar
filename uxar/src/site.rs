use argh;
use axum::body::Body;
use axum::extract::Request;
use axum::http::Uri;
use axum::{Extension, Router, ServiceExt as _};
use chrono::format;
use include_dir::{Dir, DirEntry, File};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::any::Any;
use std::collections::VecDeque;
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::{any::TypeId, collections::HashMap, path::PathBuf, sync::Arc};
use tower::{Layer as _, ServiceExt};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;

use crate::IntoApplication;
use crate::app::Application;
use crate::auth::Authenticator;
use crate::db::DbExecutor;
use crate::layers::{redirect_trailing_slash_on_404, rewrite_request_path};
use crate::{
    cmd::{NestedCommand, SiteCommand},
    conf::SiteConf,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("Database connection error: {0}")]
    DatabaseConnectionError(#[from] sqlx::Error),

    #[error("Service not found for type: {0}")]
    ServiceNotFound(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Template rendering error: {0}")]
    TemplateError(#[from] minijinja::Error),

    #[error("Serve error: {0}")]
    ServeError(#[from] axum::Error),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),
}

fn collect_files(dir: include_dir::Dir<'static>, store: &mut Vec<File<'static>>) {
    let binding = [dir];
    let mut stack: VecDeque<&include_dir::Dir<'static>> = VecDeque::from_iter(&binding);
    while let Some(current_dir) = stack.pop_front() {
        for subdir in current_dir.dirs() {
            stack.push_back(subdir);
        }
        for file in current_dir.files() {
            store.push(file.clone());
        }
    }
}

pub struct SiteBuilder {
    conf: SiteConf,
    static_files: Vec<File<'static>>,
    template_files: Vec<File<'static>>,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    router: Router<Site>,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self {
            conf,
            static_files: Vec::new(),
            template_files: Vec::new(),
            router: Router::new(),
            services: HashMap::new(),
        }
    }

    pub fn with_service<T: Service>(mut self, service: T) -> Self {
        let type_id = TypeId::of::<T>();
        self.services.insert(type_id, Arc::new(service));
        self
    }

    /// Mount an application to the site at a specific path. axum::Router can also be used as an application.
    pub fn mount<A: IntoApplication>(mut self, path: &str, app: A) -> Self {
        let app = app.into_app();
        let mut router = app.router();

        collect_files(app.static_dir(), &mut self.static_files);

        collect_files(app.template_dir(), &mut self.template_files);

        let app = Arc::new(app);
        router = router.layer(axum::middleware::from_fn(
            move |mut req: Request<Body>, next: axum::middleware::Next| {
                let app = Arc::clone(&app);
                async move {
                    req.extensions_mut().insert(Extension(app));
                    next.run(req).await
                }
            },
        ));

        if path.is_empty() {
            self.router = self.router.merge(router);
        } else {
            self.router = self.router.nest(path, router);
        }

        self
    }

    pub async fn run(self) -> Result<(), SiteError> {
        let site = self.build().await?;
        site.run().await
    }

    pub async fn build(self) -> Result<Site, SiteError> {
        let project_dir = Site::project_dir();

        let mut router = self.router;

        for static_dir in &self.conf.static_dirs {
            let path = project_dir.join(&static_dir.path);
            let serve_dir = ServeDir::new(&path).append_index_html_on_directories(false);
            router = router.route_service(&static_dir.url, serve_dir);
        }

        let mut env = minijinja::Environment::new();

        let db_url = self.conf.database.clone();
        let parts = db_url
            .parse::<url::Url>()
            .map_err(|e| SiteError::ConfigError(format!("Invalid database URL: {}", e)))?;

        let mut query = HashMap::new();
        for (key, value) in parts.query_pairs() {
            query.insert(key.to_string(), value.to_string());
        }
        let max_connections = query
            .remove("max_connections")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let min_connections = query
            .remove("min_connections")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .connect(&db_url)
            .await?;

        for file in self.template_files {
            let path = file
                .path()
                .to_str()
                .expect(&format!("Invalid UTF-8 path: {:?}", file.path()));
            let body = String::from_utf8(file.contents().to_vec())
                .expect(&format!("Invalid UTF-8 content in file: {:?}", file.path()));
            if let Err(err) = env.add_template_owned(path, body) {
                panic!("Failed to add template: {}, Error: {}", path, err);
            }
        }

        let authenticator = Authenticator::new(&self.conf.auth, &self.conf.secret_key);
        router = router
            .fallback(redirect_trailing_slash_on_404)
            .layer(CatchPanicLayer::new());

        let site = Site {
            inner: Arc::new(SiteInner {
                conf: self.conf,
                services: self.services,
                pool,
                authenticator,
                template_env: env,
                router,
            }),
        };

        Ok(site)
    }
}

pub trait Service: 'static + Send + Sync + std::any::Any {}

impl<T: Send + Sync + Any> Service for T {} // Blanket impl

#[derive(Clone, Debug)]
struct SiteInner {
    conf: SiteConf,
    authenticator: Authenticator,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    pool: PgPool,
    router: Router<Site>,
    template_env: minijinja::Environment<'static>,
}

#[derive(Debug, Clone)]
pub struct Site {
    inner: Arc<SiteInner>,
}

impl Site {
    pub fn builder(conf: SiteConf) -> SiteBuilder {
        SiteBuilder::new(conf)
    }

    pub fn render_template(
        &self,
        template_name: &str,
        context: &minijinja::value::Value,
    ) -> Result<String, minijinja::Error> {
        self.inner
            .template_env
            .get_template(template_name)?
            .render(context)
    }

    pub fn get_service<T: Service>(&self) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        self.inner
            .services
            .get(&type_id)
            .and_then(|service| service.clone().downcast::<T>().ok())
    }

    pub fn authenticator(&self) -> &Authenticator {
        &self.inner.authenticator
    }

    pub fn db(&self) -> DbExecutor<'_> {
        DbExecutor::new(&self.inner.pool)
    }

    /// Mainly needed for testing purposes.
    pub fn router(&self) -> Router {
        let router: Router = self.inner.router.clone().with_state(self.clone());
        router
    }

    fn project_dir() -> PathBuf {
        if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
            PathBuf::from(manifest_dir)
        } else {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        }
    }

    async fn serve_forver(self, verbose: bool) -> Result<(), SiteError> {
        let host = self.inner.conf.host.clone();
        let port = self.inner.conf.port;

        // Parse the address and handle errors gracefully
        let addr: SocketAddr = format!("{}:{}", host, port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut iter| iter.next())
            .ok_or_else(|| {
                SiteError::ConfigError(format!(
                    "Failed to resolve address for {}:{}. Ensure the address is valid.",
                    host, port
                ))
            })?;

        let listener = tokio::net::TcpListener::bind(addr).await?;

        if verbose {
            println!("Server running at http://{}", addr);
        }

        let service = tower::util::MapRequestLayer::new(rewrite_request_path).layer(self.router());

        axum::serve(listener, service.into_make_service())
            .with_graceful_shutdown(Self::shutdown_signal(verbose))
            .await?;

        Ok(())
    }

    async fn shutdown_signal(verbose: bool) {
        use tokio::signal;
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
        if verbose {
            println!("Signal received, starting graceful shutdown");
        }
    }

    async fn run(self) -> Result<(), SiteError> {
        let cmd: SiteCommand = argh::from_env();
        match cmd.nested {
            NestedCommand::Init(_) => {}
            NestedCommand::Serve(_) => {
                self.serve_forver(cmd.verbose).await?;
            }
        }
        Ok(())
    }
}

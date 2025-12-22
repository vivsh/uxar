use crate::app::{Application};
use crate::auth::Authenticator;
use crate::db::{DbError, DbPool};
use crate::layers::{redirect_trailing_slash_on_404, rewrite_request_path};
use crate::views::Routable;
use crate::{IntoApplication, embed, watch};
use crate::{
    views,
    cmd::{NestedCommand, SiteCommand},
    conf::SiteConf,
};
use argh;
use axum::body::Body;
use axum::extract::Request;
use axum::{Extension, Router as AxumRouter, ServiceExt as _};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::mpsc;
use std::any::Any;
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::{any::TypeId, collections::HashMap, path::PathBuf, sync::Arc};
use tower::Layer as _;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;

use std::path::Path;
use thiserror::Error; // Import the Path type

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DbError),

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


pub struct SiteBuilder {
    conf: SiteConf,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    router: views::Router,
    apps: Vec<Arc<dyn Application>>,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self {
            conf,
            apps: Vec::new(),
            router: views::Router::new(),
            services: HashMap::new(),
        }
    }

    pub fn with_service<T: Service>(mut self, service: T) -> Self {
        let type_id = TypeId::of::<T>();
        self.services.insert(type_id, Arc::new(service));
        self
    }

    /// Mount an application to the site at a specific path. axum::Router can also be used as an application.
    pub fn mount<R:  views::Routable>(mut self, path: &str, namespace: &str, app: R) -> Self {
       self.router = self.router.mount(path, namespace, app);
        self
    }

    pub fn merge<R: views::Routable>(mut self, other: R) -> Self {
        self.router = self.router.merge(other);
        self
    }

    pub async fn run(self) -> Result<(), SiteError> {
        let site = self.build().await?;
        site.run().await
    }

    fn inject_templates(
        apps: &[Arc<dyn Application>],
        conf: &SiteConf,
        project_dir: &Path,
        env: &mut minijinja::Environment,
    ) -> Result<(), SiteError> {
        let mut dir_vec = Vec::new();

        for app in apps {
            if let Some(dir) = app.templates_dir() {
                dir_vec.push(dir);
            }
        }

        // iterate over files in conf.templates_dir
        if let Some(templates_dir) = &conf.templates_dir {
            let path = crate::conf::project_dir().join(templates_dir);
            dir_vec.push(embed::Dir::Path {
                root: path.clone(),
                path,
            });
        }

        for file in embed::DirSet::new(dir_vec).walk() {
            let path = file.path();
            let name = path.to_string_lossy().to_string();
            let content = file.read_bytes_sync().map_err(|e| {
                SiteError::ConfigError(format!(
                    "Failed to read template file: {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let body = String::from_utf8(content).map_err(|e| {
                SiteError::ConfigError(format!(
                    "Invalid UTF-8 in template file: {}: {}",
                    path.display(),
                    e
                ))
            })?;
            if let Err(err) = env.add_template_owned(name, body) {
                return Err(SiteError::TemplateError(err));
            }
        }

        Ok(())
    }

    pub async fn build(self) -> Result<Site, SiteError> {
        let project_dir = PathBuf::from(&self.conf.project_dir);        ;

        let (mut router, metas) = self.router.into_router_parts();

        for static_dir in &self.conf.static_dirs {
            let path = project_dir.join(&static_dir.path);
            let serve_dir = ServeDir::new(&path).append_index_html_on_directories(false);
            router = router.nest_service(&static_dir.url, serve_dir);
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
            .remove("max")
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        let min_connections = query
            .remove("min")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .connect(&db_url)
            .await.map_err(|e| SiteError::DatabaseError(DbError::Fatal(e)))?;

        Self::inject_templates(&self.apps, &self.conf, &project_dir, &mut env)?;

        let authenticator = Authenticator::new(&self.conf.auth, &self.conf.secret_key);
        router = router
            .fallback(redirect_trailing_slash_on_404)
            .layer(CatchPanicLayer::new());

        let inspector = views::RouterMeta::new(metas);

        // let router = views::Router::from_parts(metas, router);

        let site = Site {
            inner: Arc::new(SiteInner {
                project_dir,
                start_time: std::time::Instant::now(),
                conf: self.conf,
                services: self.services,
                pool,
                authenticator,
                template_env: env,
                router,
                view_inspeector: inspector,
            }),
        };

        Ok(site)
    }
}

pub trait Service: 'static + Send + Sync + std::any::Any {}


impl<T: Send + Sync + Any> Service for T {} // Blanket impl


pub trait SignalHandler: 'static + Send + Sync {

    fn handle_signal(&self, site: &Site, object: &dyn Any);

}


#[derive(Clone, Debug)]
struct SiteInner {
    start_time: std::time::Instant,
    project_dir: PathBuf,
    conf: SiteConf,
    authenticator: Authenticator,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    pool: PgPool,
    router: axum::Router<Site>,
    template_env: minijinja::Environment<'static>,
    view_inspeector: views::RouterMeta,
}

#[derive(Debug, Clone)]
pub struct Site {
    inner: Arc<SiteInner>,
}

impl Site {
    pub fn builder(conf: SiteConf) -> SiteBuilder {
        SiteBuilder::new(conf)
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.inner.start_time.elapsed()
    }

    pub fn project_dir(&self) -> &Path {
        self.inner.project_dir.as_path()
    }

    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        self.inner.view_inspeector.reverse(name, args)
    }

    pub fn render_template<S: serde::Serialize>(
        &self,
        template_name: &str,
        context: &S,
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

    pub fn db(&self) -> DbPool<'_> {
        DbPool::new(&self.inner.pool)
    }

    /// Mainly needed for testing purposes.
    /// and before running the server.
    pub fn router(&self) -> axum::Router {
        let router = self.inner.router
            .clone();

        router.with_state(self.clone())
    }

    async fn serve_forever(self, verbose: bool) -> Result<(), SiteError> {
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

        let touch_reload = self.inner.conf.touch_reload.clone();
        
        axum::serve(listener, service.into_make_service())
            .with_graceful_shutdown(watch::shutdown_signal(touch_reload))
            .await?;

        Ok(())
    }

    pub async fn notify_stream(
        &self,
        topics: &[&str],
        capacity: usize,
    ) -> Result<mpsc::Receiver<crate::db::Notify>, SiteError> {
        let db_url = self.inner.conf.database.clone();
        let receiver = crate::db::DbPool::consume_notify(&db_url, topics, capacity)
            .await
            .map_err(|e| SiteError::DatabaseError(e))?;
        Ok(receiver)
    }

    async fn run(self) -> Result<(), SiteError> {
        if self.inner.conf.log_init{
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .init();
        }
        let cmd: SiteCommand = argh::from_env();
        match cmd.nested {
            NestedCommand::Init(_) => {}
            NestedCommand::Serve(_) => {
                self.serve_forever(cmd.verbose).await?;
            }
        }
        Ok(())
    }
}

impl axum::extract::FromRequest<Site> for Site {
    type Rejection = axum::http::StatusCode;

    // Suppress the unused variable warning for `req` in `from_request`
    #[allow(unused_variables)]
    async fn from_request(req: Request<Body>, state: &Site) -> Result<Self, Self::Rejection> {
        Ok(state.clone())
    }
}

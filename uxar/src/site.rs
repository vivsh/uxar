use crate::app::{self, Application};
use crate::auth::Authenticator;
use crate::db::DbExecutor;
use crate::layers::{redirect_trailing_slash_on_404, rewrite_request_path};
use crate::{IntoApplication, embed, watch};
use crate::{
    cmd::{NestedCommand, SiteCommand},
    conf::SiteConf,
};
use argh;
use axum::body::Body;
use axum::extract::Request;
use axum::{Extension, Router, ServiceExt as _};
use futures;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::any::Any;
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::{any::TypeId, collections::HashMap, path::PathBuf, sync::Arc};
use tower::Layer as _;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::services::ServeDir;

use std::path::Path;
use thiserror::Error; // Import the Path type

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

fn collect_files(dir: embed::Dir, store: &mut Vec<embed::File>) {
    let binding = [dir];
    let mut stack: VecDeque<embed::Dir> = VecDeque::from_iter(binding);
    while let Some(current_dir) = stack.pop_front() {
        for subdir in current_dir.entries() {
            match subdir {
                embed::Entry::File(file) => store.push(file),
                embed::Entry::Dir(subdir) => stack.push_back(subdir),
            }
        }
    }
}

/// Recursively iterates through a given directory and yields `File` objects.
pub fn iter_files_recursively<P: AsRef<Path>>(
    dir: P,
) -> impl Iterator<Item = io::Result<fs::File>> {
    fn visit_dirs(dir: &Path) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(visit_dirs(&path)?);
            } else {
                files.push(path);
            }
        }
        Ok(files)
    }

    let files = visit_dirs(dir.as_ref()).unwrap_or_else(|_| Vec::new());
    files.into_iter().map(|path| fs::File::open(path))
}

pub struct SiteBuilder {
    conf: SiteConf,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    router: Router<Site>,
    apps: Vec<Arc<dyn Application>>,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self {
            conf,
            apps: Vec::new(),
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

        let app = Arc::new(app);
        let app_clone = Arc::clone(&app);
        router = router.layer(axum::middleware::from_fn(
            move |mut req: Request<Body>, next: axum::middleware::Next| {
                let app = app_clone.clone();
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

        self.apps.push(app);

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
            let path = project_dir.join(templates_dir);
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
        let project_dir = Site::project_dir();

        let mut router = self.router;

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
            .await?;

        Self::inject_templates(&self.apps, &self.conf, &project_dir, &mut env)?;

        let authenticator = Authenticator::new(&self.conf.auth, &self.conf.secret_key);
        router = router
            .fallback(redirect_trailing_slash_on_404)
            .layer(CatchPanicLayer::new());

        let site = Site {
            inner: Arc::new(SiteInner {
                start_time: std::time::Instant::now(),
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
    start_time: std::time::Instant,
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

    pub fn uptime(&self) -> std::time::Duration {
        self.inner.start_time.elapsed()
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

        let touch_reload = self.inner.conf.touch_reload.clone();
        axum::serve(listener, service.into_make_service())
            .with_graceful_shutdown(watch::shutdown_signal(touch_reload))
            .await?;

        Ok(())
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
                self.serve_forver(cmd.verbose).await?;
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

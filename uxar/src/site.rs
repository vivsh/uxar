use crate::app::{Application};
use crate::auth::Authenticator;
use crate::db::{DbError, DbPool};
use crate::layers::{redirect_trailing_slash_on_404, rewrite_request_path};
use crate::tasks::{self, TaskError, TaskManager};
use crate::views::Routable;
use crate::{embed, watch};
use crate::{
    views,
    cmd::{NestedCommand, SiteCommand},
    conf::SiteConf,
};
use argh;
use axum::body::Body;
use axum::extract::Request;
use axum::{ServiceExt as _};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::mpsc;
use chrono_tz::Tz;
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

    #[error("Task error: {0}")]
    TaskError(#[from] TaskError),
}


type ServiceType = dyn 'static + Send + Sync + Any;

pub struct SiteBuilder {
    conf: SiteConf,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    router: views::Router,
    apps: Vec<Arc<dyn Application>>,
    lazy_db: bool,
    disable_tasks: bool,
    task_builder: tasks::TaskEngineBuilder,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self {
            conf,
            apps: Vec::new(),
            router: views::Router::new(),
            services: HashMap::new(),
            task_builder: tasks::TaskEngineBuilder::new(chrono_tz::Tz::UTC),
            lazy_db: false,
            disable_tasks: false,
        }
    }

    pub fn without_tasks(mut self) -> Self {
        self.disable_tasks = true;
        self.task_builder.clear();
        self
    }

    pub fn with_lazy_db(mut self) -> Self {
        self.lazy_db = true;
        self
    }

    pub fn with_service<T: 'static + Send + Sync>(mut self, service: impl Into<Arc<T>>) -> Self {
        let type_id = TypeId::of::<T>();
        self.services.insert(type_id, service.into());
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

    /// Register a task handler with typed arguments
    pub fn with_task<T, F, Fut>(mut self, name: &str, handler: F) -> Self
    where
        T: serde::de::DeserializeOwned + Send + Sync + 'static,
        F: Fn(Site, T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        debug_assert!(!self.disable_tasks, "Cannot register tasks when tasks are disabled");
        self.task_builder.register(name, handler);
        self
    }

    /// Schedule a task to run on a cron schedule
    pub fn cron_task<T: serde::Serialize>(
        mut self,
        name: &str,
        cron_expr: &str,
        arg: T,
    ) -> Self{
        debug_assert!(!self.disable_tasks, "Cannot register tasks when tasks are disabled");
        self.task_builder.schedule_cron(name, cron_expr, arg);
        self
    }

    /// Schedule a task to run at regular intervals (in milliseconds)
    pub fn periodic_task<T: serde::Serialize>(
        mut self,
        name: &str,
        millis: i64,
        arg: T,
    ) -> Self {
        debug_assert!(!self.disable_tasks, "Cannot register tasks when tasks are disabled");
        self.task_builder.schedule_interval(name, millis, arg);
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
        let project_dir = PathBuf::from(&self.conf.project_dir);

        let timezone = match &self.conf.tz {
            Some(tz_str) => tz_str.parse::<Tz>().map_err(|_| {
                SiteError::ConfigError(format!("Invalid timezone: {}", tz_str))
            })?,
            None => Tz::UTC,
        };

        let task_engine = self.task_builder.build_tz(timezone)?;

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

        let pool = if self.lazy_db {
            PgPoolOptions::new()
                .max_connections(max_connections)
                .min_connections(min_connections)
                .connect_lazy(&db_url)
                .map_err(|e| SiteError::DatabaseError(DbError::Fatal(e)))?
        } else {
            PgPoolOptions::new()
                .max_connections(max_connections)
                .min_connections(min_connections)
                .connect(&db_url)
                .await
                .map_err(|e| SiteError::DatabaseError(DbError::Fatal(e)))?
        };

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
                shutdown_notifier: Arc::new(tokio::sync::Notify::new()),
                timezone,
                task_engine,
                authenticator,
                template_env: env,
                router,
                router_meta: inspector,
            }),
        };

        if !self.disable_tasks {
            // Start the task runner in the background
            site.inner.task_engine.start_runner(site.clone(), site.inner.shutdown_notifier.clone()).await?;
        }

        Ok(site)
    }
}


pub struct Service<T>(pub Arc<T>);

impl<T: 'static + Send + Sync> axum::extract::FromRequestParts<Site> for Service<T> {
    type Rejection = crate::ApiError;

    async fn from_request_parts(_parts: &mut axum::http::request::Parts, state: &Site) -> Result<Self, Self::Rejection> {
        match state.get_service::<T>() {
            Some(service) => Ok(Service(service)),
            None => {
                let type_name = std::any::type_name::<T>();
                Err(crate::ApiError::internal_error().with_details(format!("Service not found: {}", type_name)))
            }
        }
    }
}

impl<T> std::ops::Deref for Service<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}



#[derive(Clone, Debug)]
struct SiteInner {
    start_time: std::time::Instant,
    project_dir: PathBuf,
    conf: SiteConf,
    authenticator: Authenticator,
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    pool: PgPool,
    timezone: Tz,
    task_engine: tasks::TaskEngine,
    router: axum::Router<Site>,
    template_env: minijinja::Environment<'static>,
    router_meta: views::RouterMeta,
    shutdown_notifier: Arc<tokio::sync::Notify>,
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
        self.inner.router_meta.reverse(name, args)
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

    pub fn get_service<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        self.inner
            .services
            .get(&type_id)
            .and_then(|service| service.clone().downcast::<T>().ok())
    }

    pub fn authenticator(&self) -> &Authenticator {
        &self.inner.authenticator
    }

    pub fn tz(&self) -> Tz {
        self.inner.timezone
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
            .with_graceful_shutdown(watch::shutdown_signal(touch_reload, self.inner.shutdown_notifier.clone()))
            .await?;

        Ok(())
    }

    pub fn tasks(&self) -> TaskManager<'_> {
        self.inner.task_engine.manager()
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

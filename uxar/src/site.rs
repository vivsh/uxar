use crate::auth::Authenticator;
use crate::bundles::{Bundle, IntoBundle};
use crate::callables::{self, PayloadData};
use crate::conf::{self, SiteConf};
use crate::db::{DbError, DbPool, Notify, Pool};
use crate::emitters::EmitTarget;
use crate::logging::{self, LoggingGuard};
use crate::notifiers::CancellationNotifier;
use crate::tasks::{TaskDispatcher, TaskError, TaskRunner, TaskStore};
use crate::templates::{TemplateEngine, TemplateError};
use crate::{beacon, embed, services, watch};
use axum::ServiceExt;
use axum::extract::Request;
use chrono_tz::Tz;
use serde::Serialize;

use std::net::{SocketAddr, ToSocketAddrs as _};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use tower::Layer as _;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::services::ServeDir;

use std::path::Path;
use thiserror::Error; // Import the Path type

#[derive(Debug, Clone)]
pub(crate) struct PartialSite {
    db: DbPool,
}

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] DbError),

    #[error("Service not found for type: {0}")]
    ServiceNotFound(String),

    #[error("Configuration error: {0}")]
    ConfError(#[from] conf::ConfError),

    #[error("Assets error: {0}")]
    AssetError(String),

    #[error("Template file error: {0}")]
    TemplateFileError(String),

    #[error("Address resolution error: {0}")]
    AddressResolutionError(String),

    #[error("Invalid timezone: {0}")]
    TimezoneError(String),

    #[error("File watch error: {0}")]
    FileWatchError(String),

    #[error(transparent)]
    BundleError(#[from] crate::bundles::BundleError),

    #[error(transparent)]
    TemplateError(#[from] TemplateError),

    #[error("Serve error: {0}")]
    ServeError(#[from] axum::Error),

    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    EmitterError(#[from] crate::emitters::EmitterError),

    #[error(transparent)]
    SignalError(#[from] crate::signals::SignalError),

    #[error(transparent)]
    LoggingError(#[from] logging::LoggingError),

    #[error(transparent)]
    ServiceError(#[from] services::ServiceError),
}

struct SiteBuilder {
    conf: SiteConf,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self { conf }
    }

    async fn start_engines(site: &Site) -> Result<(), SiteError> {
        // let task_store = site.inner.task_engine.store();
        // task_store.run_migrations().await.map_err(|e| {
        //     return SiteError::ConfigError(format!("Failed to run task store migrations: {}", e));
        // })?;

        let task_runner = TaskRunner::new(site.inner.task_engine.clone());

        let signal_site = site.clone();
        let task_site = signal_site.clone();

        site.inner.joinset.lock().spawn(async move {
            task_runner.run(task_site).await;
        });

        let emitter_engine = site.inner.emitter_engine.clone();
        site.inner.joinset.lock().spawn(async move {
            if let Err(err) = emitter_engine.run(signal_site).await {
                tracing::error!("Emitter engine error: {}", err);
            }
        });

        site.inner
            .service_engine
            .start_workers(site.clone(), &mut site.inner.joinset.lock())
            .await?;

        Ok(())
    }

    async fn start_server(site: Site) -> Result<(), SiteError> {
        let host = site.inner.conf.host.clone();
        let port = site.inner.conf.port;

        // Parse the address and handle errors gracefully
        let addr: SocketAddr = format!("{}:{}", host, port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut iter| iter.next())
            .ok_or_else(|| {
                SiteError::AddressResolutionError(format!(
                    "Failed to resolve address for {}:{}. Ensure the address is valid.",
                    host, port
                ))
            })?;

        let listener = tokio::net::TcpListener::bind(addr).await?;

        let router = site.router().layer(CatchPanicLayer::new());

        let svc = NormalizePathLayer::trim_trailing_slash().layer(router);
        let make_svc = ServiceExt::<Request>::into_make_service(svc);

        let touch_reload = site.inner.conf.touch_reload.clone();

        axum::serve(listener, make_svc)
            .with_graceful_shutdown(watch::shutdown_signal(
                touch_reload,
                site.inner.shutdown_notifier.clone(),
            ))
            .await?;

        site.inner.joinset.lock().shutdown().await;

        Ok(())
    }

    async fn build(
        &self,
        pool: Option<Pool>,
        bundle: impl IntoBundle,
    ) -> Result<SiteInner, SiteError> {
        self.conf.validate()?;

        let bundle = bundle.into_bundle();

        bundle.validate()?;

        let project_dir = PathBuf::from(&self.conf.project_dir);

        let timezone = match &self.conf.tz {
            Some(tz_str) => tz_str
                .parse::<Tz>()
                .map_err(|_| SiteError::TimezoneError(tz_str.clone()))?,
            None => Tz::UTC,
        };

        let mut router = bundle.to_router();

        for static_dir in &self.conf.static_dirs {
            let path = project_dir.join(&static_dir.path);
            let serve_dir = ServeDir::new(&path).append_index_html_on_directories(false);
            router = router.nest_service(&static_dir.url, serve_dir);
        }

        let mut template_engine = TemplateEngine::new();

        let pool = if let Some(pool) = pool {
            DbPool::from_pool(pool)
        } else {
            DbPool::from_conf(&self.conf.database).await?
        };

        template_engine.inject_templates(
            self.conf.templates_dir.as_ref().map(|s| s.as_str()),
            &bundle,
        )?;

        let authenticator = Authenticator::new(&self.conf.auth, &self.conf.secret_key);

        let bundle = bundle.with_router_unchecked(router);

        let task_store = TaskStore::new(pool.inner().clone(), 1024);

        let task_config = self.conf.tasks.clone();

        let task_registry = Arc::new(bundle.tasks.clone().with_config(task_config));

        let task_dispatcher = task_registry.dispatcher(Arc::new(task_store));

        let signal_engine = bundle.signals.engine();

        let emitter_engine = bundle.emitters.create_engine();

        let logging_guard = logging::init_tracing(&project_dir, &self.conf.logging)?;

        let mut site = SiteInner {
            _logging_guard: logging_guard,
            project_dir,
            start_time: std::time::Instant::now(),
            conf: self.conf.clone(),
            pool,
            shutdown_notifier: CancellationNotifier::new(),
            service_engine: services::ServiceEngine::new(),
            timezone,
            authenticator,
            template_engine,
            joinset: Arc::new(parking_lot::Mutex::new(tokio::task::JoinSet::new())),
            beacon: beacon::Beacon::new(128, false),
            bundle,
            signal_engine,
            emitter_engine,
            task_engine: task_dispatcher,
        };

        site.load_services().await?;

        Ok(site)
    }
}

struct SiteInner {
    start_time: std::time::Instant,
    project_dir: PathBuf,
    conf: SiteConf,
    authenticator: Authenticator,
    pool: DbPool,
    beacon: beacon::Beacon,
    template_engine: TemplateEngine,
    timezone: Tz,
    bundle: Bundle,
    signal_engine: crate::signals::SignalEngine,
    emitter_engine: crate::emitters::EmitterEngine,
    task_engine: TaskDispatcher<TaskStore>,
    service_engine: services::ServiceEngine,
    shutdown_notifier: CancellationNotifier,
    _logging_guard: LoggingGuard,
    joinset: Arc<parking_lot::Mutex<tokio::task::JoinSet<()>>>,
}

impl SiteInner {
    async fn load_services(&mut self) -> Result<(), SiteError> {
        let registry = self.bundle.services.clone();
        let partial_site = PartialSite {
            db: self.pool.clone(),
        };

        self.service_engine.load(registry, partial_site).await?;

        Ok(())
    }
}

#[derive(Clone)]
pub struct Site {
    inner: Arc<SiteInner>,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Site")
            .field("project_dir", &self.inner.project_dir)
            .field("conf", &self.inner.conf)
            .finish()
    }
}

impl Site {
    pub fn uptime(&self) -> std::time::Duration {
        self.inner.start_time.elapsed()
    }

    pub fn iter_operations(&self) -> impl Iterator<Item = &callables::Operation> {
        self.inner.bundle.iter_operations()
    }

    pub fn shutdown_notifier(&self) -> CancellationNotifier {
        self.inner.shutdown_notifier.child()
    }

    /// Notify all components to shutdown
    pub fn shutdown(&self) {
        self.inner.shutdown_notifier.notify_waiters();
    }

    /// Notify all components to shutdown
    pub async fn shutdown_and_wait(&self) {
        self.inner.shutdown_notifier.notify_waiters();
        self.inner.joinset.lock().shutdown().await
    }

    pub(crate) async fn dispatch_payload(
        &self,
        payload: PayloadData,
        target: EmitTarget,
    ) -> Result<(), SiteError> {
        match target {
            EmitTarget::Signal => {
                self.inner
                    .signal_engine
                    .dispatch_payload(self.clone(), payload)
                    .await?;
            }
            EmitTarget::Task => {
                // self.inner.task_dispatcher.submit_data(name, payload)
            }
        }
        Ok(())
    }

    pub(crate) async fn consume_notify(
        &self,
        topics: &[String],
    ) -> Result<mpsc::Receiver<Notify>, DbError> {
        let capacity = topics.len() * 100 + 10;
        self.db()
            .consume_notify(topics, capacity, self.shutdown_notifier())
            .await
    }

    pub(crate) fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        self.inner.joinset.lock().spawn(fut);
    }

    pub fn project_dir(&self) -> &Path {
        self.inner.project_dir.as_path()
    }

    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        self.inner.bundle.reverse(name, args)
    }

    pub fn render_template<S: serde::Serialize>(
        &self,
        template_name: &str,
        context: &S,
    ) -> Result<String, TemplateError> {
        self.inner.template_engine.render(template_name, context)
    }

    pub fn authenticator(&self) -> &Authenticator {
        &self.inner.authenticator
    }

    pub fn tz(&self) -> Tz {
        self.inner.timezone
    }

    pub fn db(&self) -> DbPool {
        self.inner.pool.clone()
    }

    /// Mainly needed for testing purposes.
    /// and before running the server.
    pub fn router(&self) -> axum::Router {
        let router = self.inner.bundle.to_router();
        router.with_state(self.clone())
    }

    pub fn beacon(&self) -> beacon::Beacon {
        self.inner.beacon.clone()
    }

    pub async fn submit_typed_task<T: Serialize + 'static>(
        &self,
        input: T,
    ) -> Result<uuid::Uuid, TaskError> {
        self.inner.task_engine.submit_typed(input).await
    }

    pub async fn submit_task<T: Serialize + 'static>(
        &self,
        task_name: &str,
        input: T,
    ) -> Result<uuid::Uuid, TaskError> {
        self.inner.task_engine.submit(task_name, input).await
    }

    pub async fn submit_task_data(
        &self,
        task_name: &str,
        data: String,
    ) -> Result<uuid::Uuid, TaskError> {
        self.inner.task_engine.submit_data(task_name, &data).await
    }
}

impl axum::extract::FromRequestParts<Site> for Site {
    type Rejection = axum::http::StatusCode;

    // Suppress the unused variable warning for `req` in `from_request`
    #[allow(unused_variables)]
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &Site,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.clone())
    }
}
impl axum::extract::FromRef<Site> for beacon::Beacon {
    fn from_ref(site: &Site) -> Self {
        site.beacon()
    }
}

pub async fn build_site(conf: SiteConf, bundle: impl IntoBundle) -> Result<Site, SiteError> {
    let builder = SiteBuilder::new(conf);
    let site = builder.build(None, bundle).await?;
    let site = Site {
        inner: Arc::new(site),
    };
    SiteBuilder::start_engines(&site).await?;
    Ok(site)
}

pub async fn serve_site(conf: SiteConf, bundle: impl IntoBundle) -> Result<(), SiteError> {
    let site = build_site(conf, bundle).await?;
    SiteBuilder::start_server(site).await
}

pub async fn test_site(
    conf: SiteConf,
    bundle: impl IntoBundle,
    pool: Pool,
) -> Result<Site, SiteError> {
    let builder = SiteBuilder::new(conf);
    let site = builder.build(Some(pool), bundle).await?;
    let site = Site {
        inner: Arc::new(site),
    };
    SiteBuilder::start_engines(&site).await?;
    Ok(site)
}

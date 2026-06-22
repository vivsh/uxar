use crate::auth::Authenticator;
use crate::bundles::{Bundle, IntoBundle};
use crate::callables::{self, DataBox};
use crate::channels::{ChannelRef, LocalChannelBackend};
use crate::commands::CommandRegistry;
use crate::conf::{self, SiteConf};
use crate::db::{DbError, DbPool, Notify, Pool};
use crate::emitters::EmitTarget;
use crate::logging::{self, LoggingGuard};
use crate::notifiers::CancellationNotifier;
use crate::signals::SignalClient;
#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
use crate::tasks::store::AbstractTaskStore as _;
#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
use crate::tasks::{TaskClient, TaskDispatcher, TaskError, TaskRunner, TaskStore};
use crate::templates::{TemplateEngine, TemplateError, Templates};
use crate::{services, watch};
use axum::ServiceExt;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use chrono_tz::Tz;
use std::net::{SocketAddr, ToSocketAddrs as _};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use std::path::Path;
use thiserror::Error; // Import the Path type

async fn error_report_middleware(State(site): State<Site>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let headers = req.headers().clone();
    let response = next.run(req).await;
    let Some(report) = response
        .extensions()
        .get::<crate::errors::ErrorReport>()
        .cloned()
    else {
        return response;
    };
    let ctx = crate::errors::ErrorContext {
        method,
        uri,
        path,
        headers,
    };
    site.inner.conf.errors.render(ctx, report).await
}

#[derive(Debug, Clone)]
pub(crate) struct PartialSite {
    db: DbPool,
}

impl PartialSite {
    pub(crate) fn db(&self) -> DbPool {
        self.db.clone()
    }
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

    #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
    #[error("Task migration error: {0}")]
    TaskMigrationError(#[from] TaskError),

    #[error(transparent)]
    CommandError(#[from] crate::commands::CommandError),
}

struct SiteBuilder {
    conf: SiteConf,
}

impl SiteBuilder {
    fn new(conf: SiteConf) -> Self {
        Self { conf }
    }

    async fn start_engines(site: &Site) -> Result<(), SiteError> {
        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        {
            if site.inner.task_engine.has_tasks() {
                let task_runner = TaskRunner::new(site.inner.task_engine.clone());

                let signal_site = site.clone();
                let task_site = signal_site.clone();

                site.inner.joinset.lock().spawn(async move {
                    task_runner.run(task_site).await;
                });
            }
        }

        let signal_site = site.clone();

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

        let make_svc = ServiceExt::<Request>::into_make_service(site.router());

        let touch_reload = site.inner.conf.touch_reload.clone();

        axum::serve(listener, make_svc)
            .with_graceful_shutdown(watch::shutdown_signal(
                touch_reload,
                site.inner.shutdown_notifier.clone(),
            ))
            .await?;

        // Abort all tasks and wait for them to finish without holding the lock
        // across an await point (parking_lot guards are not Send).
        site.inner.joinset.lock().abort_all();
        while let Some(_) = site.inner.joinset.lock().try_join_next() {}

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

        template_engine.inject_templates(&self.conf.templates.dirs, &project_dir, &bundle)?;

        let authenticator =
            Authenticator::new(&self.conf.auth, &self.conf.secret_key, &project_dir)
                .map_err(|err| conf::ConfError::Other(format!("Auth config error: {err}")))?;

        bundle.doc_engine.setup(&mut router, &bundle.ops)?;

        let slash_router = Arc::new(
            crate::middlewares::SlashRouter::from_operations(
                bundle.ops.values().cloned(),
                self.conf.http.slash.policy,
            )
            .map_err(crate::bundles::BundleError::DocGen)?,
        );

        let mut bundle = bundle.with_router_unchecked(router);

        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        let task_config = self.conf.tasks.clone();

        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        let task_store = TaskStore::new(
            pool.as_sqlx().clone(),
            task_config.batch_size,
            std::time::Duration::from_millis(task_config.lease_duration_ms as u64),
        );

        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        let task_registry = Arc::new(bundle.tasks.clone().with_config(task_config));

        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        let task_dispatcher = task_registry.dispatcher(Arc::new(task_store));

        let signal_engine = bundle.signals.engine();

        let emitter_engine = bundle
            .emitters
            .create_engine_with_conf(self.conf.emitters.clone());

        let mut command_registry = std::mem::replace(
            &mut bundle.commands,
            crate::commands::CommandRegistry::new(),
        );
        command_registry.merge(crate::commands::builtin_registry()?)?;

        let logging_guard = if self.conf.log_init {
            logging::init_tracing(&project_dir, &self.conf.logging)?
        } else {
            LoggingGuard::noop()
        };

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
            slash_router,
            joinset: Arc::new(parking_lot::Mutex::new(tokio::task::JoinSet::new())),
            channels: LocalChannelBackend::new(self.conf.channels.clone()),
            bundle,
            signal_engine,
            emitter_engine,
            commands: command_registry,
            #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
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
    channels: LocalChannelBackend,
    template_engine: TemplateEngine,
    slash_router: Arc<crate::middlewares::SlashRouter>,
    timezone: Tz,
    bundle: Bundle,
    signal_engine: crate::signals::SignalEngine,
    emitter_engine: crate::emitters::EmitterEngine,
    commands: CommandRegistry,
    #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
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

#[derive(Debug, Clone)]
pub struct SiteConfig(SiteConf);

impl SiteConfig {
    pub fn into_inner(self) -> SiteConf {
        self.0
    }
}

impl std::ops::Deref for SiteConfig {
    type Target = SiteConf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<SiteConf> for SiteConfig {
    fn as_ref(&self) -> &SiteConf {
        &self.0
    }
}

impl callables::FromSite for SiteConfig {
    fn from_site(site: &Site) -> Result<Self, callables::CallError> {
        Ok(Self(site.conf().clone()))
    }
}

impl callables::IntoArgPart for SiteConfig {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
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
    pub async fn build(conf: SiteConf, bundle: impl IntoBundle) -> Result<Self, SiteError> {
        let builder = SiteBuilder::new(conf);
        let site = builder.build(None, bundle).await?;
        let site = Self {
            inner: Arc::new(site),
        };
        #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
        if site.inner.task_engine.has_tasks() {
            site.inner.task_engine.store().run_migrations().await?;
        }
        SiteBuilder::start_engines(&site).await?;
        Ok(site)
    }

    pub async fn run(conf: SiteConf, bundle: impl IntoBundle) -> Result<(), SiteError> {
        Self::run_with_args(conf, bundle, std::env::args().skip(1)).await
    }

    pub async fn serve(conf: SiteConf, bundle: impl IntoBundle) -> Result<(), SiteError> {
        let site = Self::build(conf, bundle).await?;
        site.start().await
    }

    pub async fn test(
        conf: SiteConf,
        bundle: impl IntoBundle,
        pool: Pool,
    ) -> Result<Self, SiteError> {
        let builder = SiteBuilder::new(conf);
        let site = builder.build(Some(pool), bundle).await?;
        let site = Self {
            inner: Arc::new(site),
        };
        SiteBuilder::start_engines(&site).await?;
        Ok(site)
    }

    pub async fn start(self) -> Result<(), SiteError> {
        SiteBuilder::start_server(self).await
    }

    pub(crate) async fn run_with_args(
        conf: SiteConf,
        bundle: impl IntoBundle,
        args: impl IntoIterator<Item = String>,
    ) -> Result<(), SiteError> {
        let args: Vec<String> = args.into_iter().collect();
        let (command_name, command_args) = Self::command_from_args(&args);
        let site = Self::build(conf, bundle).await?;
        let command_arg_refs: Vec<&str> = command_args.iter().map(String::as_str).collect();
        if let Err(err) = site.execute_command(&command_name, &command_arg_refs).await {
            let output = site.inner.conf.errors.render_command(
                crate::errors::ErrorCommandContext {
                    command: command_name,
                    args: command_args,
                },
                err.to_view(),
            );
            eprintln!("{output}");
            std::process::exit(1);
        }
        Ok(())
    }

    pub(crate) fn command_from_args(args: &[String]) -> (String, Vec<String>) {
        match args.first().map(String::as_str) {
            None => ("serve".to_string(), Vec::new()),
            Some("help" | "--help" | "-h") => ("help".to_string(), Vec::new()),
            Some(name) => (name.to_string(), args[1..].to_vec()),
        }
    }

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
        self.inner.joinset.lock().abort_all();
        while let Some(_) = self.inner.joinset.lock().try_join_next() {}
    }

    pub fn signals(&self) -> SignalClient {
        SignalClient::new(self.clone(), self.inner.signal_engine.clone())
    }

    pub(crate) async fn dispatch_payload(
        &self,
        payload: DataBox,
        target: EmitTarget,
    ) -> Result<(), SiteError> {
        match target {
            EmitTarget::Signal => {
                self.inner
                    .signal_engine
                    .dispatch_data_fire_and_forget(self.clone(), payload)
                    .await;
            }
            EmitTarget::Task => {
                let _ = payload;
            }
        }
        Ok(())
    }

    pub(crate) async fn consume_notify(
        &self,
        topics: &[String],
    ) -> Result<mpsc::Receiver<Notify>, DbError> {
        let conf = &self.inner.conf.emitters;
        self.db()
            .consume_notify(
                topics,
                conf.notify_channel_capacity(),
                conf.pgnotify_reconnect_initial_ms(),
                conf.pgnotify_reconnect_max_ms(),
                self.shutdown_notifier(),
            )
            .await
    }

    pub(crate) fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        self.inner.joinset.lock().spawn(fut);
    }

    pub fn project_dir(&self) -> &Path {
        self.inner.project_dir.as_path()
    }

    pub fn conf(&self) -> &SiteConf {
        &self.inner.conf
    }

    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        self.inner.bundle.reverse(name, args)
    }

    pub fn templates(&self) -> Templates {
        Templates::new(self.clone())
    }

    pub(crate) fn template_engine(&self) -> &TemplateEngine {
        &self.inner.template_engine
    }

    pub fn auth(&self) -> &Authenticator {
        &self.inner.authenticator
    }

    pub fn timezone(&self) -> Tz {
        self.inner.timezone
    }

    pub fn db(&self) -> DbPool {
        self.inner.pool.clone()
    }

    pub fn channels(&self) -> ChannelRef {
        ChannelRef::new(self.inner.channels.clone())
    }

    pub fn file_storage(&self) -> crate::file_storage::LocalStorage {
        crate::file_storage::LocalStorage::from_conf(
            &self.inner.project_dir,
            &self.inner.conf.uploads,
        )
    }

    pub fn service<T: ?Sized + 'static>(&self) -> Result<Arc<T>, services::ServiceError> {
        self.inner
            .service_engine
            .get::<T>()
            .ok_or_else(|| services::ServiceError::NotFound(std::any::type_name::<T>().to_string()))
    }

    /// Mainly needed for testing purposes.
    /// and before running the server.
    pub(crate) fn router(&self) -> axum::Router {
        let http = &self.inner.conf.http;
        let mut router = self.inner.bundle.to_router();

        router = router.layer(axum::middleware::from_fn_with_state(
            self.inner.slash_router.clone(),
            crate::middlewares::slash_middleware,
        ));

        if http.security_headers.enabled {
            router = router.layer(axum::middleware::from_fn_with_state(
                http.security_headers.clone(),
                crate::middlewares::security_headers_middleware,
            ));
        }

        if http.body_limit.enabled {
            router = router.layer(axum::middleware::from_fn_with_state(
                http.body_limit.clone(),
                crate::middlewares::body_limit_middleware,
            ));
        }

        if http.timeout.enabled {
            router = router.layer(axum::middleware::from_fn_with_state(
                http.timeout.clone(),
                crate::middlewares::timeout_middleware,
            ));
        }

        if http.request_id.enabled {
            router = router.layer(axum::middleware::from_fn_with_state(
                http.request_id.clone(),
                crate::middlewares::request_id_middleware,
            ));
        }

        if http.cors.enabled && http.cors.permissive {
            router = router.layer(CorsLayer::permissive());
        }

        if http.compression.enabled {
            router = router.layer(CompressionLayer::new());
        }

        if http.trace.enabled {
            router = router.layer(TraceLayer::new_for_http());
        }

        if http.catch_panic.enabled {
            router = router.layer(CatchPanicLayer::new());
        }

        router = router.layer(axum::middleware::from_fn_with_state(
            self.clone(),
            error_report_middleware,
        ));

        router.with_state(self.clone())
    }

    #[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
    pub fn tasks(&self) -> TaskClient<TaskStore> {
        TaskClient::new(self.inner.task_engine.clone())
    }

    pub async fn execute_command(
        &self,
        name: &str,
        args: &[&str],
    ) -> Result<(), crate::commands::CommandError> {
        self.inner.commands.execute(name, args, self.clone()).await
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

impl axum::extract::FromRequestParts<Site> for SiteConfig {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &Site,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(state.conf().clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::Site;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn command_from_empty_args_defaults_to_serve() {
        let (command, args) = Site::command_from_args(&[]);

        assert_eq!(command, "serve");
        assert!(args.is_empty());
    }

    #[test]
    fn command_from_help_args_selects_help() {
        for input in [strings(&["help"]), strings(&["--help"]), strings(&["-h"])] {
            let (command, args) = Site::command_from_args(&input);

            assert_eq!(command, "help");
            assert!(args.is_empty());
        }
    }

    #[test]
    fn command_from_named_args_preserves_command_args() {
        let input = strings(&["greet", "--name", "Vyuh"]);
        let (command, args) = Site::command_from_args(&input);

        assert_eq!(command, "greet");
        assert_eq!(args, strings(&["--name", "Vyuh"]));
    }
}

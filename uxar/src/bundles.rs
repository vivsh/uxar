use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use axum::{handler::Handler, routing::MethodRouter};
use bytes::Bytes;

use crate::{
    Site,
    callables::{self},
    commands::{self, CommandRegistry},
    embed, emitters,
    routes::{self},
    schemables::{ApiDocError, ApiDocGenerator},
    services::{Agent, Service, ServiceBuildContext, ServiceHandler, ServiceRegistry},
    signals::{self, SignalError, SignalRegistry},
    tasks::{TaskRegistry, TaskService},
};

pub use uxar_macros::{
    asset_dir, bundle, cron, flow, openapi, periodic, pgnotify, route, service, signal, task,
};

pub use {
    crate::routes::RouteConf,
    crate::schemables::{ApiMeta, DocViewer},
    emitters::CronConf,
    emitters::PeriodicConf,
    emitters::PgNotifyConf,
    signals::SignalConf,
};

#[derive(Debug, thiserror::Error, Clone)]
pub enum BundleError {
    #[error(transparent)]
    Signal(#[from] Arc<SignalError>),

    #[error(transparent)]
    Task(#[from] Arc<crate::tasks::TaskError>),

    #[error(transparent)]
    Emitter(#[from] Arc<crate::emitters::EmitterError>),

    #[error(transparent)]
    Service(#[from] Arc<crate::services::ServiceError>),

    #[error(transparent)]
    Command(Arc<crate::commands::CommandError>),

    #[error("Multiple errors occurred: {0:?}")]
    ErrorList(Vec<BundleError>),
}

#[derive(Clone, Debug)]
pub struct OpenApiConf {
    pub doc_path: String,
    pub spec_path: String,
    pub meta: ApiMeta,
    pub viewer: DocViewer,
}

enum BundlePartInner {
    Route(
        axum::routing::MethodRouter<Site>,
        crate::callables::Operation,
    ),
    OpenApi(OpenApiConf),
    Emitter(emitters::Emitter),
    Task(TaskService),
    Signal(signals::Signaller),
    Error(BundleError),
    Merge(Bundle),
    Nest(String, String, Bundle),
    Tags(Vec<Cow<'static, str>>),
    AssetDir(embed::Dir),
    Data(Arc<dyn std::any::Any + Send + Sync>),
    Command(commands::Command),
    Service(ServiceHandler),
}

pub trait IntoBundle {
    fn into_bundle(self) -> Bundle;
}

impl IntoBundle for Bundle {
    fn into_bundle(self) -> Bundle {
        self
    }
}

impl IntoBundle for axum::Router<Site> {
    fn into_bundle(self) -> Bundle {
        Bundle {
            inner_router: self,
            ..Bundle::new()
        }
    }
}

/// Bundle is a collection of routes, models, services and event handlers
/// that can be registered with the application.
pub struct Bundle {
    inner_router: routes::AxumRouter<Site>,
    meta_map: BTreeMap<String, crate::callables::Operation>,
    operations: Vec<crate::callables::Operation>,
    pub(crate) signals: SignalRegistry,
    pub(crate) emitters: emitters::EmitterRegistry,
    errors: Vec<BundleError>,
    pub(crate) tasks: TaskRegistry,
    pub(crate) asset_dirs: Vec<embed::Dir>,
    pub(crate) services: ServiceRegistry,
    pub(crate) commands: CommandRegistry,
}

impl Bundle {
    /// Creates a new empty Bundle.
    pub fn new() -> Self {
        Self {
            inner_router: routes::AxumRouter::new(),
            operations: Vec::new(),
            meta_map: BTreeMap::new(),
            signals: SignalRegistry::new(),
            emitters: emitters::EmitterRegistry::new(),
            errors: Vec::new(),
            tasks: TaskRegistry::new(),
            asset_dirs: Vec::new(),
            services: ServiceRegistry::new(),
            commands: CommandRegistry::new(),
        }
    }

    pub fn validate(&self) -> Result<(), BundleError> {
        if !self.errors.is_empty() {
            return Err(BundleError::ErrorList(self.errors.clone()));
        }
        Ok(())
    }

    fn with_api_doc(mut self, api_path: &str, doc_path: &str, viewer: DocViewer) -> Self {
        let doc_html = ApiDocGenerator::serve_doc(api_path, viewer);
        let body_bytes = Bytes::from(doc_html.0);
        let handler = move || {
            let body = body_bytes.clone();
            async move {
                use axum::http::{StatusCode, header};
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                    body,
                )
            }
        };

        self.inner_router = self.inner_router.route(doc_path, routes::get(handler));
        self
    }

    fn with_api_spec_and_doc(
        mut self,
        api_path: &str,
        doc_path: &str,
        meta: ApiMeta,
        viewer: DocViewer,
    ) -> Self {
        self = self.with_api_spec(api_path, meta);
        self.with_api_doc(api_path, doc_path, viewer)
    }

    fn with_api_spec(mut self, path: &str, meta: ApiMeta) -> Self {
        let doc_bytes = match self.create_openapi(meta) {
            Ok(json) => Bytes::from(json),
            Err(e) => {
                eprintln!("Failed to generate OpenAPI: {}", e);
                Bytes::from_static(b"{\"error\": \"Failed to generate OpenAPI\"}")
            }
        };

        let handler = move || {
            let body = doc_bytes.clone();
            async move {
                use axum::http::{StatusCode, header};
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    body,
                )
            }
        };

        self.inner_router = self.inner_router.route(path, routes::get(handler));
        self
    }

    /// Generate OpenAPI documentation for this bundle
    fn create_openapi(&self, meta: ApiMeta) -> Result<String, ApiDocError> {
        let views: Vec<&crate::callables::Operation> = self.meta_map.values().collect();
        let doc_gen = ApiDocGenerator::new(meta);
        let api = doc_gen.generate(&views)?;
        serde_json::to_string(&api).map_err(ApiDocError::JsonSerialization)
    }

    pub fn to_router(&self) -> routes::AxumRouter<Site> {
        self.inner_router.clone()
    }

    pub fn iter_routes(&self) -> impl Iterator<Item = &crate::callables::Operation> {
        self.meta_map.values()
    }

    pub fn iter_operations(&self) -> impl Iterator<Item = &crate::callables::Operation> {
        // chain operations as well as meta_map::values()
        self.operations.iter().chain(self.meta_map.values())
    }

    fn add_tags(self, _tags: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) -> Self {
        // TODO: apply tags to operations once Operation has a `tags` field
        self
    }

    /// Sets the inner router directly, replacing any existing routes.
    /// This is an unsafe operation as it may overwrite existing routes.
    /// Use with caution.
    /// Meant for advanced use cases where you need full control over the router e.g. to add layers
    pub fn with_router_unchecked(mut self, router: routes::AxumRouter<Site>) -> Self {
        self.inner_router = router;
        self
    }

    fn merge_non_route_parts(&mut self, other: Bundle) -> Result<axum::Router<Site>, BundleError> {
        self.meta_map.extend(other.meta_map);
        self.operations.extend(other.operations);
        self.signals.merge(other.signals);
        self.asset_dirs.extend(other.asset_dirs);
        if let Err(e) = self.emitters.merge(other.emitters) {
            return Err(BundleError::Emitter(Arc::new(e)));
        }
        if let Err(e) = self.services.merge(other.services) {
            return Err(BundleError::Service(Arc::new(e)));
        }
        if let Err(e) = self.tasks.merge(other.tasks) {
            return Err(BundleError::Task(Arc::new(e)));
        }
        if let Err(e) = self.commands.merge(other.commands) {
            return Err(BundleError::Command(Arc::new(e)));
        }
        Ok(other.inner_router)
    }

    fn merge<B: IntoBundle>(mut self, other: B) -> Self {
        let other = other.into_bundle();
        let router = match self.merge_non_route_parts(other) {
            Ok(r) => r,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };
        self.inner_router = self.inner_router.merge(router);
        self
    }

    fn nest<B: IntoBundle>(mut self, path: &str, namespace: &str, other: B) -> Self {
        debug_assert!(!path.ends_with('/'), "Mount path should not end with '/'");
        debug_assert!(
            path.starts_with('/'),
            "Mount path should always start with '/'"
        );
        let mut other = other.into_bundle();
        for meta in other.meta_map.values_mut() {
            meta.nest(path, namespace);
        }
        let other_router = match self.merge_non_route_parts(other) {
            Ok(r) => r,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };

        self.inner_router = self.inner_router.nest(path, other_router);
        self
    }

    fn route<H, T>(mut self, handler: H, operation: crate::callables::Operation) -> Self
    where
        H: Handler<T, Site> + Send + Sync + 'static,
        T: 'static,
    {
        let method_router: MethodRouter<Site> =
            axum::routing::on(operation.methods.into(), handler);

        self.inner_router = self
            .inner_router
            .route(operation.path.as_ref(), method_router);

        self.meta_map.insert(operation.name.to_string(), operation);
        self
    }

    /// Iterate over all registered views' metadata in no insertion order
    pub fn iter_views(&self) -> impl Iterator<Item = &crate::callables::Operation> {
        self.meta_map.values()
    }

    /// Reverse lookup a URL by view name and parameters
    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        let meta = self.meta_map.get(name)?;
        let mut path = meta.path.to_string();

        for (k, v) in args {
            // Support both axum styles:
            // Only support the `{name}` style (axum v2+).
            let brace = format!("{{{}}}", k);
            if path.contains(&brace) {
                path = path.replace(&brace, v);
            }
        }

        // Optional safety check (debug only): no unfilled "{param}" left
        debug_assert!(
            !path.contains('{'),
            "reverse() for '{}' called with missing args; remaining template: {}",
            name,
            path
        );

        Some(path)
    }

    pub fn from_parts(parts: impl IntoIterator<Item = BundlePart>) -> Self {
        let mut bundle = Bundle::new();
        for part in parts {
            bundle = bundle.inject_part(part);
        }
        bundle
    }

    fn inject_part(mut self, part: BundlePart) -> Self {
        if !matches!(&part.part, BundlePartInner::Route(..)) {
            if let Some(op) = part.operation {
                self.operations.push(op);
            }
        }
        match part.part {
            BundlePartInner::Route(router, op) => {
                self = self.route(router, op);
            }
            BundlePartInner::Emitter(em) => {
                if let Err(e) = self.emitters.register(em) {
                    self.errors.push(BundleError::Emitter(Arc::new(e)));
                }
            }
            BundlePartInner::Signal(sig) => {
                self.signals.register(sig);
            }
            BundlePartInner::Error(e) => {
                self.errors.push(e);
            }
            BundlePartInner::Merge(b) => {
                self = self.merge(b);
            }
            BundlePartInner::Nest(path, namespace, b) => {
                self = self.nest(&path, &namespace, b);
            }
            BundlePartInner::AssetDir(d) => {
                self.asset_dirs.push(d);
            }
            BundlePartInner::Tags(tags) => {
                self = self.add_tags(tags);
            }
            BundlePartInner::Service(entry) => {
                if let Err(e) = self.services.register(entry) {
                    self.errors.push(BundleError::Service(Arc::new(e)));
                }
            }
            BundlePartInner::OpenApi(conf) => {
                self = self.with_api_spec_and_doc(
                    &conf.spec_path,
                    &conf.doc_path,
                    conf.meta,
                    conf.viewer,
                );
            }
            BundlePartInner::Task(ts) => {
                if let Err(e) = self.tasks.register(ts) {
                    self.errors.push(BundleError::Task(Arc::new(e)));
                }
            }
            BundlePartInner::Command(cmd) => {
                if let Err(e) = self.commands.register(cmd) {
                    self.errors.push(BundleError::Command(Arc::new(e)));
                }
            }
            BundlePartInner::Data(_) => {}
        }
        self
    }
}

pub struct BundlePart {
    part: BundlePartInner,
    operation: Option<crate::callables::Operation>,
}

impl BundlePart {
    pub fn patch(mut self, f: callables::PatchOp) -> Self {
        if let Some(op) = &mut self.operation {
            f.apply(op);
        } else if let BundlePartInner::Route(_, ref mut op) = self.part {
            f.apply(op);
        }
        self
    }
}

pub fn route<H, T, Args>(handler: H, meta: RouteConf) -> BundlePart
where
    H: axum::handler::Handler<T, Site> + callables::Specable<Args> + Clone + Send + Sync + 'static,
    T: 'static,
    Args: callables::IntoArgSpecs + 'static,
{
    let spec = callables::CallSpec::new(&handler);
    let mut operation =
        crate::callables::Operation::from_specs(crate::callables::OperationKind::Route, &spec);

    operation.path = meta.path.clone().into();
    operation.name = meta.name.clone().into();
    operation.methods = meta.methods.clone().into();
    operation = operation.with_conf(&meta);

    let route = axum::routing::on(meta.methods.into(), handler);
    BundlePart {
        operation: None,
        part: BundlePartInner::Route(route, operation),
    }
}

pub fn cron<O, H, Args>(handler: H, options: emitters::CronConf) -> BundlePart
where
    O: callables::Payloadable,
    Args:
        callables::FromContext<emitters::EmitterContext> + callables::IntoArgSpecs + Send + 'static,
    H: callables::Specable<Args, Output = callables::Payload<O>> + Send + Sync + 'static,
{
    let (op, part) = match emitters::cron(handler, options) {
        Ok(emitter) => (Some(emitter.operation()), BundlePartInner::Emitter(emitter)),
        Err(e) => (
            None,
            BundlePartInner::Error(BundleError::Emitter(Arc::new(e))),
        ),
    };

    BundlePart {
        part,
        operation: op,
    }
}

pub fn service<T, H, Args>(handler: H) -> BundlePart
where
    T: Service,
    H: callables::Specable<Args, Output = Agent<T>> + Send + Sync + 'static,
    Args: callables::FromContext<ServiceBuildContext> + callables::IntoArgSpecs + Send + 'static,
{
    let entry = ServiceHandler::new(handler);
    BundlePart {
        part: BundlePartInner::Service(entry),
        operation: None,
    }
}

pub fn periodic<O, H, Args>(handler: H, options: emitters::PeriodicConf) -> BundlePart
where
    O: callables::Payloadable,
    Args:
        callables::FromContext<emitters::EmitterContext> + callables::IntoArgSpecs + Send + 'static,
    H: callables::Specable<Args, Output = callables::Payload<O>> + Send + Sync + 'static,
{
    let (op, part) = match emitters::periodic(handler, options) {
        Ok(emitter) => (Some(emitter.operation()), BundlePartInner::Emitter(emitter)),
        Err(e) => (
            None,
            BundlePartInner::Error(BundleError::Emitter(Arc::new(e))),
        ),
    };

    BundlePart {
        part,
        operation: op,
    }
}

pub fn pgnotify<O, H, Args>(handler: H, options: emitters::PgNotifyConf) -> BundlePart
where
    O: callables::Payloadable,
    Args:
        callables::FromContext<emitters::EmitterContext> + callables::IntoArgSpecs + Send + 'static,
    H: callables::Specable<Args, Output = callables::Payload<O>> + Send + Sync + 'static,
{
    let (op, part) = match emitters::pgnotify(handler, options) {
        Ok(emitter) => (Some(emitter.operation()), BundlePartInner::Emitter(emitter)),
        Err(e) => (
            None,
            BundlePartInner::Error(BundleError::Emitter(Arc::new(e))),
        ),
    };

    BundlePart {
        part,
        operation: op,
    }
}

pub fn signal<T, H, Args>(handler: H, options: SignalConf) -> BundlePart
where
    T: callables::Payloadable,
    H: callables::Specable<Args, Output = ()> + Send + Sync + 'static,
    Args: callables::FromContext<signals::SignalContext>
        + callables::IntoArgSpecs
        + callables::HasPayload<T>
        + Send
        + 'static,
{
    let part = BundlePartInner::Signal(crate::signals::signal::<T, H, Args>(handler, options));

    BundlePart {
        part,
        operation: None,
    }
}

pub fn command<T, H, Args>(handler: H, conf: commands::CommandConf) -> BundlePart
where
    T: callables::Payloadable,
    H: callables::Specable<Args, Output = Result<(), commands::CommandError>>
        + Send
        + Sync
        + 'static,
    Args: callables::FromContext<commands::CommandContext>
        + callables::IntoArgSpecs
        + callables::HasPayload<T>
        + Send
        + 'static,
{
    let part = BundlePartInner::Command(commands::command(handler, conf));

    BundlePart {
        part,
        operation: None,
    }
}

pub fn merge<B: IntoBundle>(other: B) -> BundlePart {
    let part = BundlePartInner::Merge(other.into_bundle());
    BundlePart {
        part,
        operation: None,
    }
}

pub fn nest<B: IntoBundle>(path: &str, namespace: &str, other: B) -> BundlePart {
    let part = BundlePartInner::Nest(path.to_string(), namespace.to_string(), other.into_bundle());
    BundlePart {
        part,
        operation: None,
    }
}

pub fn openapi(conf: OpenApiConf) -> BundlePart {
    let part = BundlePartInner::OpenApi(conf);
    BundlePart {
        part,
        operation: None,
    }
}

pub fn tags(tags: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) -> BundlePart {
    let part = BundlePartInner::Tags(tags.into_iter().map(|t| t.into()).collect());
    BundlePart {
        part,
        operation: None,
    }
}

pub fn asset_dir(dir: embed::Dir) -> BundlePart {
    let part = BundlePartInner::AssetDir(dir);
    BundlePart {
        part,
        operation: None,
    }
}

pub fn bundle(parts: impl IntoIterator<Item = BundlePart>) -> Bundle {
    Bundle::from_parts(parts)
}

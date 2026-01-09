use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use axum::{handler::Handler, response::IntoResponse, routing::MethodRouter};
use bytes::Bytes;
use futures::future::BoxFuture;

use crate::{
    Site,
    schemables::{ApiDocGenerator, ApiMeta, DocViewer, Schemable},
    signals::{SignalEngine, SignalError},
    views::{self, JsonStr, ViewMeta},
};

pub use uxar_macros::{bundle_impl, bundle_routes, route};

pub trait IntoBundle {
    fn into_bundle(self) -> Bundle;
}

impl IntoBundle for Bundle {
    fn into_bundle(self) -> Bundle {
        self
    }
}

impl IntoBundle for (axum::routing::Router<Site>, Vec<ViewMeta>) {
    fn into_bundle(self) -> Bundle {
        let (router, metas) = self;
        let bundle = Bundle::__internal_from_parts(router, metas, Vec::new());
        bundle
    }
}

/// Bundle is a collection of routes, models, services and event handlers
/// that can be registered with the application.
#[derive(Clone, Debug)]
pub struct Bundle {
    inner_router: views::AxumRouter<Site>,
    meta_map: BTreeMap<String, ViewMeta>,
    signals: SignalEngine,
}

impl Bundle {
    /// Creates a new empty Bundle.
    pub fn new() -> Self {
        Self {
            inner_router: views::AxumRouter::new(),
            meta_map: BTreeMap::new(),
            signals: SignalEngine::new(),
        }
    }

    pub fn with_api_doc(mut self, api_path: &str, doc_path: &str, viewer: DocViewer) -> Self {
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

        self.inner_router = self
            .inner_router
            .route(doc_path, views::get(handler));
        self
    }

    pub fn with_api_spec_and_doc(
        mut self,
        api_path: &str,
        doc_path: &str,
        meta: ApiMeta,
        viewer: DocViewer,
    ) -> Self {
        self = self.with_api_spec(api_path, meta);
        self.with_api_doc(api_path, doc_path, viewer)
    }

    pub fn with_api_spec(mut self, path: &str, meta: ApiMeta) -> Self {
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

        self.inner_router = self.inner_router.route(path, views::get(handler));
        self
    }

    /// Generate OpenAPI documentation for this bundle
    pub fn create_openapi(&self, meta: ApiMeta) -> Result<String, serde_json::Error> {
        let views: Vec<&ViewMeta> = self.meta_map.values().collect();
        let doc_gen = ApiDocGenerator::new(meta);
        let api = doc_gen.generate(&views);
        serde_json::to_string(&api)
    }

    /// Internal constructor for use by macros only.
    /// Users should use `Bundle::new()` and builder methods instead.
    #[doc(hidden)]
    pub fn __internal_from_parts(
        router: views::AxumRouter<Site>,
        metas: Vec<ViewMeta>,
        tags: Vec<Cow<'static, str>>,
    ) -> Self {
        let mut meta_map = BTreeMap::new();
        
        for mut m in metas {
            if !tags.is_empty() {
                m.tags.reserve(tags.len());
                m.tags.extend(tags.iter().cloned());
            }
            meta_map.insert(m.name.to_string(), m);
        }
        
        Self {
            inner_router: router,
            meta_map,
            signals: SignalEngine::new(),
        }
    }

    pub fn to_router(&self) -> views::AxumRouter<Site> {
        self.inner_router.clone()
    }

    /// Apply additional tags to all routes in this bundle.
    /// The tags are appended to each route's existing tags.
    pub fn with_tags(
        mut self,
        tags: impl IntoIterator<Item = impl Into<Cow<'static, str>>>,
    ) -> Self {
        let iter = tags.into_iter();
        let (lower, _) = iter.size_hint();
        let mut tag_vec = Vec::with_capacity(lower);
        tag_vec.extend(iter.map(|t| t.into()));
        
        for meta in self.meta_map.values_mut() {
            meta.tags.reserve(tag_vec.len());
            meta.tags.extend(tag_vec.iter().cloned());
        }
        self
    }

    /// Sets the inner router directly, replacing any existing routes.
    /// This is an unsafe operation as it may overwrite existing routes.
    /// Use with caution.
    /// Meant for advanced use cases where you need full control over the router e.g. to add layers
    pub fn with_router_unchecked(mut self, router: views::AxumRouter<Site>) -> Self {
        self.inner_router = router;
        self
    }

    pub fn on_signal<F, T>(mut self, name: &'static str, receiver: F) -> Self
    where
        F: Fn(Site, T) -> BoxFuture<'static, Result<(), SignalError>> + Send + Sync + 'static,
        T: Schemable + Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        self.signals
            .register(name, move |site, payload| receiver(site, payload));
        self
    }

    pub fn merge<B: IntoBundle>(mut self, other: B) -> Self {
        let other = other.into_bundle();
        self.inner_router = self.inner_router.merge(other.inner_router);
        self.meta_map.extend(other.meta_map);
        self.signals.merge(other.signals);
        self
    }

    pub fn nest<B: IntoBundle>(mut self, path: &str, namespace: &str, other: B) -> Self {
        let other = other.into_bundle();
        self.inner_router = self.inner_router.nest(path, other.inner_router);
        let metas = other.meta_map;
        debug_assert!(!path.ends_with('/'), "Mount path should not end with '/'");
        debug_assert!(
            path.starts_with('/'),
            "Mount path should always start with '/'"
        );
        for mut meta in metas.into_values() {
            let name = format!("{}:{}", namespace, meta.name);
            let path = format!("{}{}", path, meta.path);
            meta.path = Cow::Owned(path);
            meta.name = Cow::Owned(name.clone());
            self.meta_map.insert(name, meta);
        }
        self.signals.merge(other.signals);
        self
    }

    pub fn route<H, T>(mut self, handler: H, meta: ViewMeta) -> Self
    where
        H: Handler<T, Site> + Clone + Send + Sync + 'static,
        T: 'static,
    {
        let method_router: MethodRouter<Site> = axum::routing::on(meta.method_filter, handler);

        self.inner_router = self.inner_router.route(meta.path.as_ref(), method_router);

        self.meta_map.insert(meta.name.to_string(), meta);
        self
    }

    /// Iterate over all registered views' metadata in no insertion order
    pub fn iter_views(&self) -> impl Iterator<Item = &ViewMeta> {
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

    /// Dispatch a signal to all registered handlers sequentially
    pub async fn dispatch<T>(&self, site: Site, name: &str, payload: T) -> Result<(), SignalError>
    where
        T: Send + Sync + 'static,
    {
        self.signals.dispatch(site, name, payload).await
    }

    /// Dispatch a signal, spawning each handler in a separate tokio task
    pub fn dispatch_spawn<T>(&self, site: Site, name: &str, payload: T) -> Result<(), SignalError>
    where
        T: Send + Sync + 'static,
    {
        let data = crate::signals::SignalPayload::new(payload);
        self.signals.dispatch_spawn(site, name, data)
    }

    /// Get the signal engine for direct access
    pub fn signals(&self) -> &SignalEngine {
        &self.signals
    }
}

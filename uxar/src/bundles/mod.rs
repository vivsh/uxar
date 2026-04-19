mod error;
mod openapi;
mod part;

use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

use axum::{handler::Handler, routing::MethodRouter};

use crate::{
    Site,
    callables::{self},
    commands::{self, CommandRegistry},
    embed, emitters,
    routes::{self},
    services::{ServiceRegistry},
    signals::{self, SignalRegistry},
    tasks::{TaskRegistry},
};

// ---------------------------------------------------------------------------
// Public re-exports
// ---------------------------------------------------------------------------

use openapi::DocEngine;

pub use error::BundleError;
pub use openapi::OpenApiConf;
pub use part::{BundlePart, asset_dir, bundle, command, cron, periodic, pgnotify, route, service, signal};

pub use uxar_macros::{
    asset_dir, bundle, cron, flow, periodic, pgnotify, route, service, signal, task,
};

pub use {
    crate::routes::RouteConf,
    crate::apidocs::{ApiMeta, DocViewer},
    emitters::CronConf,
    emitters::PeriodicConf,
    emitters::PgNotifyConf,
    signals::SignalConf,
};

// ---------------------------------------------------------------------------
// IntoBundle
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Bundle
// ---------------------------------------------------------------------------

/// A composable collection of HTTP routes, background services, emitters,
/// signals, commands, and asset directories that form one logical unit of
/// the application.
///
/// Build one with [`bundle()`], then compose with [`merge`], [`with_prefix`],
/// [`layer`], [`with_tags`], and [`with_openapi`].
///
/// [`merge`]: Bundle::merge
/// [`with_prefix`]: Bundle::with_prefix
/// [`layer`]: Bundle::layer
/// [`with_tags`]: Bundle::with_tags
/// [`with_openapi`]: Bundle::with_openapi
pub struct Bundle {
    pub(super) inner_router: routes::AxumRouter<Site>,
    /// All operations keyed by their stable UUID — routes, signals, tasks, commands,
    /// services, and hidden doc-engine markers alike.
    pub(crate) ops: BTreeMap<uuid::Uuid, crate::callables::Operation>,
    /// Secondary index: route name → UUID. Populated only for HTTP routes so that
    /// `reverse()` can look up a path by the human-readable name.
    pub(crate) name_index: BTreeMap<String, uuid::Uuid>,
    pub(super) id: uuid::Uuid,
    label: Option<String>,
    pub(crate) signals: SignalRegistry,
    pub(crate) emitters: emitters::EmitterRegistry,
    pub(super) errors: Vec<BundleError>,
    pub(crate) tasks: TaskRegistry,
    pub(crate) asset_dirs: Vec<embed::Dir>,
    pub(crate) services: ServiceRegistry,
    pub(crate) commands: CommandRegistry,
    pub(crate) doc_engine: DocEngine,
}

impl Bundle {
    /// Creates a new empty bundle.
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            label: None,
            inner_router: routes::AxumRouter::new(),
            ops: BTreeMap::new(),
            name_index: BTreeMap::new(),
            signals: SignalRegistry::new(),
            emitters: emitters::EmitterRegistry::new(),
            errors: Vec::new(),
            tasks: TaskRegistry::new(),
            asset_dirs: Vec::new(),
            services: ServiceRegistry::new(),
            commands: CommandRegistry::new(),
            doc_engine: DocEngine::new(),
        }
    }

    /// Returns the unique identifier for this bundle.
    pub fn id(&self) -> uuid::Uuid {
        self.id
    }

    /// Returns the label assigned via [`with_label`], if any.
    ///
    /// [`with_label`]: Bundle::with_label
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Assigns a human-readable label to this bundle (e.g. for logging or admin UIs).
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Validates that no errors were accumulated during bundle construction.
    pub fn validate(&self) -> Result<(), BundleError> {
        if !self.errors.is_empty() {
            return Err(BundleError::ErrorList(self.errors.clone()));
        }
        Ok(())
    }

    /// Returns the axum router for the HTTP routes registered in this bundle.
    pub fn to_router(&self) -> routes::AxumRouter<Site> {
        self.inner_router.clone()
    }

    /// Iterates over all operations: routes, signals, tasks, commands, services, and
    /// internal hidden markers. Callers that display operations should filter `op.hidden`.
    pub fn iter_operations(&self) -> impl Iterator<Item = &crate::callables::Operation> {
        self.ops.values()
    }

    /// Appends tags to every operation in this bundle.
    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) -> Self {
        let tags: Vec<Cow<'static, str>> = tags.into_iter().map(|t| t.into()).collect();
        for op in self.ops.values_mut() {
            op.tags.extend(tags.iter().cloned());
        }
        self
    }

    /// Merges another bundle into this one.
    ///
    /// Routes, operations, services, signals, emitters, tasks, and commands from
    /// `other` are all absorbed. Use `.merge(other.with_prefix("/path"))` to
    /// compose with a prefix.
    ///
    /// Any errors (e.g. duplicate registrations) are accumulated and surface the
    /// next time `validate()` is called. `SiteBuilder::build` always calls
    /// `validate()` before the site starts, so no error is silently swallowed.
    pub fn merge<B: IntoBundle>(mut self, other: B) -> Self {
        let other = other.into_bundle();
        let router = match self.absorb(other) {
            Ok(r) => r,
            Err(e) => {
                self.errors.push(e);
                return self;
            }
        };
        self.inner_router = self.inner_router.merge(router);
        self
    }

    /// Prefixes all routes and operation paths in this bundle with `path`.
    ///
    /// `path` must start with `'/'` and must not end with `'/'`.
    /// All operations — including hidden doc-engine markers — are updated by the
    /// same loop, so `DocEngine::setup` always sees fully-qualified paths.
    pub fn with_prefix(mut self, path: &str) -> Self {
        debug_assert!(path.starts_with('/'), "prefix must start with '/'");
        debug_assert!(!path.ends_with('/'), "prefix must not end with '/'");

        for op in self.ops.values_mut() {
            op.nest(path);
        }
        self.inner_router = routes::AxumRouter::new().nest(path, self.inner_router);
        self
    }

    /// Applies a Tower middleware layer to all routes in this bundle.
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: tower::Service<axum::http::Request<axum::body::Body>>
            + Clone
            + Send
            + Sync
            + 'static,
        <L::Service as tower::Service<axum::http::Request<axum::body::Body>>>::Response:
            axum::response::IntoResponse + 'static,
        <L::Service as tower::Service<axum::http::Request<axum::body::Body>>>::Error:
            Into<std::convert::Infallible> + 'static,
        <L::Service as tower::Service<axum::http::Request<axum::body::Body>>>::Future:
            Send + 'static,
    {
        self.inner_router = self.inner_router.layer(layer);
        self
    }

    /// Replaces the inner axum router directly.
    ///
    /// Used by `SiteBuilder` to inject the fully-configured router back into the
    /// bundle after doc-engine routes have been mounted. Not part of the public API.
    pub(crate) fn with_router_unchecked(mut self, router: routes::AxumRouter<Site>) -> Self {
        self.inner_router = router;
        self
    }

    /// Reverse-resolves a named route to its URL, filling in path parameters.
    ///
    /// Returns `None` if no route with that name is registered.
    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        let id = self.name_index.get(name)?;
        let op = self.ops.get(id)?;
        let mut path = op.path.to_string();
        for (k, v) in args {
            let placeholder = format!("{{{k}}}");
            if path.contains(&placeholder) {
                path = path.replace(&placeholder, v);
            }
        }
        debug_assert!(
            !path.contains('{'),
            "reverse('{}') called with missing args; remaining: {path}",
            name
        );
        Some(path)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Consumes `other`, absorbing all non-router state into `self`.
    /// Returns the other bundle's router so the caller can merge it.
    fn absorb(&mut self, other: Bundle) -> Result<axum::Router<Site>, BundleError> {
        self.ops.extend(other.ops);
        self.name_index.extend(other.name_index);
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
        self.doc_engine.merge(other.doc_engine);
        Ok(other.inner_router)
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

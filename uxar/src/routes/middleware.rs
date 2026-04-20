use axum::routing::Route;

use crate::callables::LayerSpec;

/// A documented tower middleware that can inject its spec into wrapped operations.
///
/// Implement this trait to make a tower layer visible in OpenAPI/apidocs output.
/// Return `Some(spec)` from [`layer_spec`] to describe what the middleware reads
/// from or adds to requests. One middleware produces at most one [`LayerSpec`];
/// multiple extraction points (e.g. reading both `Authorization` and `X-Api-Key`)
/// are represented as multiple [`ArgPart`]s inside that single spec.
///
/// Use [`RawLayer`] / [`layer_from`] to wrap any third-party tower layer that
/// needs no documentation.
///
/// [`layer_spec`]: Middleware::layer_spec
/// [`ArgPart`]: crate::callables::ArgPart
pub trait Middleware: Sized {
    /// The underlying tower layer type.
    type Layer: tower::Layer<Route> + Clone + Send + Sync + 'static;

    /// Returns the documentation spec for this middleware, or `None` if undocumented.
    ///
    /// The default implementation returns `None`.
    fn layer_spec(&self) -> Option<LayerSpec> {
        None
    }

    /// Consumes this middleware and returns the underlying tower layer.
    fn into_layer(self) -> Self::Layer;
}

/// Zero-cost adapter that wraps any tower layer as an undocumented [`Middleware`].
///
/// Use [`layer_from`] to construct one.
pub struct RawLayer<L>(pub L);

/// Wraps a plain tower layer as an undocumented [`Middleware`].
///
/// No spec is injected into operations; use this for third-party layers
/// (e.g. `tower_http::compression::CompressionLayer`) that have no
/// uxar-level documentation.
pub fn layer_from<L>(l: L) -> RawLayer<L> {
    RawLayer(l)
}

impl<L> Middleware for RawLayer<L>
where
    L: tower::Layer<Route> + Clone + Send + Sync + 'static,
{
    type Layer = L;

    fn layer_spec(&self) -> Option<LayerSpec> {
        None
    }

    fn into_layer(self) -> Self::Layer {
        self.0
    }
}

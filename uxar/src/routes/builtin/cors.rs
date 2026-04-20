use crate::callables::{ArgPart, LayerSpec, TypeSchema};
use crate::routes::middleware::Middleware;

/// CORS middleware wrapping [`tower_http::cors::CorsLayer`].
///
/// Apply to a bundle to add CORS response headers to all routes.
/// The injected [`LayerSpec`] documents the `Origin` request header
/// that the middleware validates.
///
/// # Example
///
/// ```ignore
/// use uxar::routes::CorsMiddleware;
/// use tower_http::cors::CorsLayer;
///
/// let bundle = my_bundle()
///     .layer(CorsMiddleware::new(CorsLayer::permissive()));
/// ```
pub struct CorsMiddleware(tower_http::cors::CorsLayer);

impl CorsMiddleware {
    /// Wraps a configured [`tower_http::cors::CorsLayer`].
    pub fn new(layer: tower_http::cors::CorsLayer) -> Self {
        Self(layer)
    }
}

impl Middleware for CorsMiddleware {
    type Layer = tower_http::cors::CorsLayer;

    fn layer_spec(&self) -> Option<LayerSpec> {
        Some(LayerSpec {
            name: "origin".to_string(),
            description: Some(
                "Cross-Origin Resource Sharing (CORS). \
                 Validates the Origin request header and sets \
                 Access-Control-* response headers."
                    .to_string(),
            ),
            parts: vec![ArgPart::Header(TypeSchema::wrap::<String>())],
        })
    }

    fn into_layer(self) -> Self::Layer {
        self.0
    }
}

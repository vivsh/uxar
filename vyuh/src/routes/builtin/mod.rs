#[cfg(feature = "cors")]
mod cors;

#[cfg(feature = "cors")]
pub use cors::CorsMiddleware;

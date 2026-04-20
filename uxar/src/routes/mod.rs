mod methods;
mod types;
pub mod middleware;

#[cfg(feature = "cors")]
pub mod builtin;

// Response types
pub use axum::response::{AppendHeaders, Html, IntoResponse, Json, NoContent, Redirect, Response};

// Routing helpers
pub use axum::routing::{any, delete, get, method_routing::MethodRouter, patch, post, put,
    Router as AxumRouter};

// Core extractors
pub use axum::extract::{Extension, FromRequest, FromRequestParts, MatchedPath, OriginalUri,
    Path, RawQuery, Request, State};

// Extra extractors
pub use axum_extra::extract::{Form, Multipart, Query, TypedHeader};

// HTTP primitives
pub use axum::http::{HeaderMap, HeaderName, Method as HttpMethod, StatusCode, Uri};

// Body types
pub use axum::body::Body;

// Local types
pub use methods::{MethodIter, Methods};
pub use types::{JsonStr, RouteConf};
pub use middleware::{Middleware, RawLayer, layer_from};

#[cfg(feature = "cors")]
pub use builtin::CorsMiddleware;

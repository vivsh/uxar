use std::borrow::Cow;

use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::methods::Methods;

/// Returns a string body with `application/json` content type.
///
/// Intentionally lightweight — assumes the inner value is already valid JSON.
/// Useful for returning pre-serialized JSON strings without additional overhead.
#[derive(Debug, Clone)]
pub struct JsonStr {
    inner: Cow<'static, str>,
}

impl From<&'static str> for JsonStr {
    fn from(value: &'static str) -> Self {
        Self { inner: Cow::Borrowed(value) }
    }
}

impl From<String> for JsonStr {
    fn from(value: String) -> Self {
        Self { inner: Cow::Owned(value) }
    }
}

impl IntoResponse for JsonStr {
    fn into_response(self) -> Response {
        let mut res = self.inner.into_owned().into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        res
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RouteConf {
    /// Logical name (used for reverse URLs, docs, etc.)
    pub name: Cow<'static, str>,
    /// HTTP methods supported by this view.
    pub methods: Methods,
    /// Full path, including base path if any (e.g. "/api/users/{id}").
    pub path: Cow<'static, str>,
}

impl Default for RouteConf {
    fn default() -> Self {
        Self {
            name: Cow::Borrowed(""),
            methods: Methods::GET,
            path: Cow::Borrowed("/"),
        }
    }
}

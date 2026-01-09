use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use axum::http::header;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::routing::MethodFilter;
pub use axum::response::{Html, IntoResponse, Json, Response};
pub use axum::routing::{delete, get, patch, post, put, Router as AxumRouter};

pub use uxar_macros::{bundle_routes, bundle_impl, route};


use crate::Site;
use crate::schemables::ApiFragment;
use crate::schemables::SchemaType;

/// Returns a string body with `application/json` content type.
/// This is intentionally lightweight and assumes the inner value is already valid JSON.
/// Useful for returning pre-serialized JSON strings without additional overhead.
#[derive(Debug, Clone)]
pub struct JsonStr<'a> {
    inner: Cow<'a, str>,
}

impl<'a> From<&'a str> for JsonStr<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            inner: Cow::Borrowed(value),    
        }
    }
}


impl From<String> for JsonStr<'_> {
    fn from(value: String) -> Self {
        Self {
            inner: Cow::Owned(value),
        }
    }
}


impl IntoResponse for JsonStr<'_> {
    fn into_response(self) -> Response {
        let mut res = self.inner.into_owned().into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        res
    }
}



#[derive(Debug, Clone)]
pub struct ParamMeta {
    pub name: Cow<'static, str>,
    pub fragments: Vec<ApiFragment>,
}


#[derive(Debug, Clone)]
pub struct ReturnMeta {
    pub status: Option<u16>,
    pub fragments: Vec<ApiFragment>,
}


#[derive(Debug, Clone)]
pub struct ViewMeta {
    /// Logical name (used for reverse URLs, docs, etc.)
    pub name: Cow<'static, str>,

    /// HTTP methods supported by this view as a MethodFilter
    pub method_filter: MethodFilter,

    /// HTTP methods for documentation/display purposes
    pub methods: Vec<Method>,

    /// Full path, including base path if any (e.g. "/api/users/{id}")
    pub path: Cow<'static, str>,

    /// Short one-line summary
    pub summary: Option<Cow<'static, str>>,

    /// Longer description (supports markdown)
    pub description: Option<Cow<'static, str>>,

    /// Tags for grouping/organizing endpoints in documentation
    pub tags: Vec<Cow<'static, str>>,

    /// Parameter list with metadata and schemas
    pub params: Vec<ParamMeta>,

    /// Responses map: HTTP status -> metadata
    pub responses: Vec<ReturnMeta>,
}

impl Default for ViewMeta {
    fn default() -> Self {
        Self {
            name: Cow::Borrowed(""),
            method_filter: MethodFilter::GET,
            methods: vec![],
            path: Cow::Borrowed("/"),
            summary: None,
            description: None,
            tags: vec![],
            params: vec![],
            responses: vec![],
        }
    }
}

// Needed for macro implementation for decorators on the impl block
// Old Router and RouterMeta removed - use Bundle instead

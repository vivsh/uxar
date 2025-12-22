use std::borrow::Cow;
use std::collections::BTreeMap;

use axum::http::Method;
pub use axum::response::{Html, IntoResponse, Json, Response};
pub use axum::routing::{delete, get, patch, post, put, Router as AxumRouter};

pub use uxar_macros::{route, routable};

use crate::Site;



/// A part of a JSON Schema (e.g. for a field, object, etc.)
/// This is a placeholder for now.
/// In a real implementation, it would contain the actual schema representation.
/// This will be used for automatic API documentation, validation, etc.

#[derive(Debug, Clone)]
pub struct SchemaPart{

}

pub trait IntoSchemaPart{
    fn into_schema_part() -> Option<SchemaPart>;
}

pub struct NilSchema<T>(pub T);

impl<T> IntoSchemaPart for NilSchema<T>{
    fn into_schema_part() -> Option<SchemaPart> {
        None
    }
}

// Basic implementations for common types
impl IntoSchemaPart for i32 {
    fn into_schema_part() -> Option<SchemaPart> {
        Some(SchemaPart {})
    }
}

impl IntoSchemaPart for i64 {
    fn into_schema_part() -> Option<SchemaPart> {
        Some(SchemaPart {})
    }
}

impl IntoSchemaPart for String {
    fn into_schema_part() -> Option<SchemaPart> {
        Some(SchemaPart {})
    }
}

impl IntoSchemaPart for bool {
    fn into_schema_part() -> Option<SchemaPart> {
        Some(SchemaPart {})
    }
}

impl IntoSchemaPart for Response {
    fn into_schema_part() -> Option<SchemaPart> {
        Some(SchemaPart {})
    }
}



#[derive(Debug, Clone, Default)]
pub struct ParamMeta {
    pub name: Cow<'static, str>,
    pub type_name: Cow<'static, str>,
    pub schema: Option<SchemaPart>,
    /// Optional source/location: e.g. "path", "query", "body"
    pub source: Option<Cow<'static, str>>,
}


#[derive(Debug, Clone, Default)]
pub struct ReturnMeta {
    pub status: Option<u16>,
    pub type_name: Cow<'static, str>,
    pub schema: Option<SchemaPart>,
}

#[derive(Debug, Clone, Default)]
pub struct ViewMeta {
    /// Logical name (used for reverse URLs, docs, etc.)
    pub name: Cow<'static, str>,

    /// HTTP methods supported by this view (GET/POST/...)
    pub methods: Vec<Method>,

    /// Full path, including base path if any (e.g. "/api/users/{id}")
    pub path: Cow<'static, str>,

    /// Short one-line summary/description
    pub summary: Option<Cow<'static, str>>,

    /// Parameter list with metadata and schemas
    pub params: Vec<ParamMeta>,

    /// Responses map: HTTP status -> metadata
    pub responses: Vec<ReturnMeta>,
}

// Needed for macro implementation for decorators on the impl block
pub trait StaticRoutable{
    fn as_routable() -> (axum::Router<Site>, Vec<ViewMeta>);
}

pub trait Routable {
    fn into_router_parts(self) -> (axum::Router<Site>, Vec<ViewMeta>);
}

impl Routable for axum::Router<Site> {
    fn into_router_parts(self) -> (axum::Router<Site>, Vec<ViewMeta>) {
        (self, Vec::new())
    }
}

impl Routable for (axum::Router<Site>, Vec<ViewMeta>) {
    fn into_router_parts(self) -> (axum::Router<Site>, Vec<ViewMeta>) {
        (self.0, self.1)
    }
}

impl Routable for Router {
    fn into_router_parts(self) -> (axum::Router<Site>, Vec<ViewMeta>) {
        (self.base_router, self.meta_map.values().cloned().collect())
    }
}


#[derive(Debug)]
pub struct Router {
    meta_map: BTreeMap<String, ViewMeta>,
    pub(crate) base_router: axum::Router<Site>,
}

impl Clone for Router {
    fn clone(&self) -> Self {
        Self {
            meta_map: self.meta_map.clone(),
            base_router: self.base_router.clone(),
        }
    }
}

impl Router {

    pub fn new() -> Self {
        Self {
            meta_map: BTreeMap::new(),
            base_router: axum::Router::new(),
        }
    }

    pub fn from_parts(metas: Vec<ViewMeta>, router: axum::Router<Site>) -> Self {
        let mut meta_map = BTreeMap::new();
        for meta in metas {
            meta_map.insert(meta.name.to_string(), meta);
        }
        Self {
            meta_map,
            base_router: router,
        }
    }

    pub fn merge<R: Routable>(mut self, other: R) -> Self {
        let (router, metas) = other.into_router_parts();
        for meta in metas {
            self.meta_map.insert(meta.name.to_string(), meta);
        }
        self.base_router = self.base_router.merge(router);
        self
    }

    /// Mount a Routable item (view, sub-router, etc.) at the given path and namespace
    /// - `path`: Base path to mount at (e.g. "/api/users").
    ///     -  It should start with '/' but not end with '/'.
    /// - `namespace`: Logical namespace for the mounted item (used for reverse URL lookups, etc.)
    ///     - It should only contain alphanumeric characters and underscores.
    ///     - It will be used as a prefix (with a colon) for all view names within the mounted item.
    ///     - e.g. "api_users:another_view"
    ///  As of now, these constraints are not enforced at all for meta
    pub fn mount<R: Routable>(mut self, path: &str, namespace: &str, item: R) -> Self {
        debug_assert!(
            !path.ends_with('/'),
            "Mount path should not end with '/'"
        );
        debug_assert!(
            path.starts_with('/'),
            "Mount path should always start with '/'"
        );
        let (router, metas) = item.into_router_parts();
        for mut meta in metas {
            let name = format!("{}:{}", namespace, meta.name);
            let path = format!("{}{}", path, meta.path);
            meta.path = Cow::Owned(path);
            meta.name = Cow::Owned(name.clone());
            self.meta_map.insert(name, meta);
        }
        self.base_router = self.base_router.nest(path, router);

        self
    }

    pub fn make_inspector(&self) -> RouterMeta {
        RouterMeta::new(self.meta_map.values().cloned().collect())
    }

}


/// container for all view metadata
#[derive(Debug, Clone)]
pub struct RouterMeta{
    map: BTreeMap<String, ViewMeta>,
}

impl RouterMeta{

    pub fn new(metas: Vec<ViewMeta>) -> Self{
        let mut map = BTreeMap::new();
        for meta in metas{
            map.insert(meta.name.to_string(), meta);
        }
        Self{
            map,
        }
    }

    /// Iterate over all registered views' metadata in no insertion order
    pub fn iter_views(&self) -> impl Iterator<Item = &ViewMeta> {
        self.map.values()
    }

    /// Reverse lookup a URL by view name and parameters
    pub fn reverse(&self, name: &str, args: &[(&str, &str)]) -> Option<String> {
        let meta = self.map.get(name)?;
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

}
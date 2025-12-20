

use axum::{http::Method};
pub use axum::routing::{get, post, put, delete, patch, Router};
pub use axum::response::{IntoResponse, Response, Json, Html};

pub use uxar_macros::{viewable, action};

pub type RouterType = Router;

pub trait Viewable{

    fn describe_routes() -> Vec<ViewMeta>;

    fn register_routes(router: axum::Router) -> axum::Router;
}


#[derive(Debug, Clone)]
pub struct ParamMeta {
    pub name: &'static str,
    pub ty:   &'static str,
}

#[derive(Debug, Clone)]
pub struct ViewMeta {
    /// Logical name (used for reverse URLs, docs, etc.)
    pub name: &'static str,

    /// HTTP method (GET/POST/...)
    pub method: Method,

    /// Full path, including base path if any (e.g. "/api/users/{id}")
    pub path: &'static str,

    /// Short one-line summary/description
    pub summary: Option<&'static str>,

    /// Parameter list: *(name, type)* as strings
    pub params: &'static [ParamMeta],

    /// Return type as string (e.g. "impl IntoResponse", "Result<Json<User>, ApiError>")
    pub return_type: &'static str,
}

pub enum ComponentRole{
    
}

pub struct Component{

}
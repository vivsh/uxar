mod site;
mod conf;
mod cmd;
mod app;
mod host;
mod migrations;
mod errors;
mod auth;
mod roles;
pub mod validation;
mod validators;
mod layers;
mod tasks;
mod watch;
pub mod db;
pub mod embed;
pub mod testing;

mod defer;

pub mod views;
pub use site::{Site, SiteError};
pub use conf::{SiteConf, StaticDir};
pub use app::{Application, IntoApplication};
pub use host::{HostService};
pub use axum::extract::{FromRequest, Json, Path, State, FromRequestParts};
pub use axum_extra::extract::{Query, Form, TypedHeader, Multipart};
pub use auth::{AuthUser, AuthConf, AuthError};pub use validation::{Valid, ValidRejection, Validate, ValidationReport, ValidationError};

// Re-export proc macros
pub use uxar_macros::Validatable;
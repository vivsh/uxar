mod site;
mod conf;
mod cmd;
mod app;
mod host;
mod migrations;
mod jsql;
mod errors;
mod auth;
mod roles;
mod validation;
mod views;
mod layers;
mod watch;
pub mod db;
pub mod embed;
pub mod testing;
pub mod cron;

mod defer;


pub use site::{Site, SiteError};
pub use conf::{SiteConf, StaticDir};
pub use app::{Application, IntoApplication};
pub use host::{HostService};
pub use axum::extract::{FromRequest, Json, Path, State, FromRequestParts};
pub use axum_extra::extract::{Query, Form, TypedHeader, Multipart};
pub use auth::{AuthUser, AuthConf, AuthError};


pub mod commons{
    pub use chrono;
    pub use tokio;
    pub use tower;
    pub use axum;
    pub use dotenvy;    
    pub use uuid;
    pub use serde;
    pub use axum_extra;
    pub use serde_json;
    pub use strum;
    pub use strum_macros;
    pub use utoipa;
    pub use garde;
    pub use thiserror;
    pub use axum_test;
}
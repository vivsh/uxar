use std::collections::HashMap;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use axum_jsonschema::JsonSchemaRejection;
use serde::Serialize;
use thiserror::Error;


#[derive(Debug, Error)]
pub enum SiteError {
    #[error("Permission denied")]
    PermissionDenied,

    #[error("Not found")]
    NotFound,

    #[error("Authentication required")]
    Unauthorized,

    #[error("Validation error: {0}")]
    GardeError(#[from] garde::Report),

    #[error("Suspicious operation")]
    SuspiciousOperation,

    #[error("Improperly configured")]
    ImproperlyConfigured,

    #[error("Other error: {0}")]
    Other(anyhow::Error),
}

impl IntoResponse for SiteError {
    fn into_response(self) -> Response {
        match self {
            SiteError::GardeError(report) => {
                let mut errors = HashMap::<String, Vec<String>>::new();
                for e in report.iter() {
                    errors
                        .entry(e.0.to_string())
                        .or_default()
                        .push(e.1.to_string());
                }
                (StatusCode::UNPROCESSABLE_ENTITY, Json(errors)).into_response()
            }
            SiteError::NotFound => (
                StatusCode::NOT_FOUND,
                "The requested resource was not found",
            )
                .into_response(),
            SiteError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Authentication is required to access this resource",
            )
                .into_response(),
            SiteError::PermissionDenied => {
                (StatusCode::FORBIDDEN, "Permission denied").into_response()
            }
            SiteError::SuspiciousOperation => {
                (StatusCode::BAD_REQUEST, "Suspicious operation").into_response()
            }
            SiteError::ImproperlyConfigured => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Improperly configured").into_response()
            }
            SiteError::Other(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Other error").into_response()
            }
        }
    }
}

pub type SiteResult<T> = Result<T, SiteError>;




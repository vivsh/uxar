use std::{borrow::Cow, fmt::Display};

use axum::{
    http::{StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use http_serde::status_code;
use serde::Serialize;
use thiserror::Error;

use crate::db::DbError;



#[derive(Debug, Clone)]
pub struct Reason{
    pub code: &'static str,
    pub status: StatusCode,
}


#[derive(Serialize)]
pub struct Problem<'a> {
    #[serde(with = "status_code")]
    pub status: StatusCode,
    pub code: &'a str,
    pub message: Cow<'a, str>,
     #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

pub trait ToProblem<'a> {
    fn to_problem(self) -> Problem<'a>;
}

impl<'a, T> ToProblem<'a> for T
where
    T: Into<Reason> + Display,
{
    fn to_problem(self) -> Problem<'a> {
        let message = self.to_string();
        let reason: Reason = self.into(); // clone required if self is not Copy
        Problem {
            status: reason.status,
            code: reason.code,
            message: Cow::Owned(message),
            data: None,
        }
    }
}

impl<'a> IntoResponse for Problem<'a> {
    fn into_response(self) -> Response {

        let status: StatusCode = self.status;
        let body = Json(&self);
        (status, body).into_response()
    }
}





impl From<DbError> for Reason {
    fn from(e: DbError) -> Self {
        let code = e.code();
        let status = e.status_code();
        Self { code, status }
    }
}


#[derive(Debug, Error)]
pub enum SiteError {
    #[error(transparent)]
    Db(#[from] DbError),

    #[error("permission denied")]
    PermissionDenied,

    #[error("authentication required")]
    Unauthorized,

    #[error("validation error")]
    ValidationError (garde::Error),

    #[error("improperly configured")]
    ImproperlyConfigured,

    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl SiteError {
    pub const fn code(&self) -> &'static str {
        match self {
            SiteError::Unauthorized => "unauthorized",
            SiteError::Db(err) => err.code(),
            SiteError::PermissionDenied => "permission_denied",
            SiteError::ImproperlyConfigured => "improperly_configured",
            SiteError::Internal(_) => "internal_error",
            SiteError::ValidationError(_) => "validation_error",
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            SiteError::Unauthorized => StatusCode::UNAUTHORIZED,
            SiteError::Db(err) => err.status_code(),
            SiteError::PermissionDenied => StatusCode::FORBIDDEN,
            SiteError::ImproperlyConfigured => StatusCode::INTERNAL_SERVER_ERROR,
            SiteError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SiteError::ValidationError(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl From<SiteError> for Reason {
    fn from(e: SiteError) -> Self {
        let code = e.code();
        let status = e.status_code();
        Self { code, status }
    }
}



pub type SiteResult<T> = Result<T, SiteError>;




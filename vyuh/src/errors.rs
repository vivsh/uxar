use crate::auth::AuthError;
use crate::db::DbError;
use crate::validation::{ValidationError, ValidationReport};
use axum::{
    Json,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use smallvec::SmallVec;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::{borrow::Cow, error::Error as StdError};

/// Transport-facing source category for rendered error reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSourceKind {
    Framework,
    Parse,
    Validation,
    Auth,
    Database,
    Template,
    Application,
    Other,
}

/// Normalized error payload used for route/extractor/application failures.
///
/// `ErrorReport` is intentionally transport-oriented. Application errors can
/// convert into it, but should not use it as their domain error type.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorReport {
    #[serde(skip_serializing)]
    pub status: StatusCode,
    pub source: ErrorSourceKind,
    pub code: Cow<'static, str>,
    pub detail: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ErrorContext {
    pub method: Method,
    pub uri: Uri,
    pub path: String,
    pub headers: HeaderMap,
}

pub type ErrorHandler = Arc<
    dyn Fn(ErrorContext, ErrorReport) -> Pin<Box<dyn Future<Output = Response> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone, Default)]
pub struct ErrorConf {
    handler: Option<ErrorHandler>,
}

impl std::fmt::Debug for ErrorConf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErrorConf")
            .field("handler", &self.handler.as_ref().map(|_| "<custom>"))
            .finish()
    }
}

impl ErrorConf {
    pub fn handler<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(ErrorContext, ErrorReport) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response> + Send + 'static,
    {
        self.handler = Some(Arc::new(move |ctx, report| Box::pin(handler(ctx, report))));
        self
    }

    pub(crate) async fn render(&self, ctx: ErrorContext, report: ErrorReport) -> Response {
        if let Some(handler) = &self.handler {
            return handler(ctx, report).await;
        }
        report.into_response()
    }
}

impl ErrorReport {
    pub fn new(
        status: StatusCode,
        source: ErrorSourceKind,
        code: impl Into<Cow<'static, str>>,
        detail: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            status,
            source,
            code: code.into(),
            detail: detail.into(),
            errors: None,
        }
    }

    pub fn bad_request(detail: impl Into<Cow<'static, str>>) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ErrorSourceKind::Parse,
            "bad_request",
            detail,
        )
    }

    pub fn validation(report: ValidationReport) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            source: ErrorSourceKind::Validation,
            code: Cow::Borrowed("validation_error"),
            detail: Cow::Borrowed("Validation failed."),
            errors: Some(report.to_nested_map()),
        }
    }

    pub fn with_errors(mut self, errors: serde_json::Value) -> Self {
        self.errors = Some(errors);
        self
    }
}

impl IntoResponse for ErrorReport {
    fn into_response(self) -> Response {
        let status = self.status;
        let mut response = (status, Json(self.clone())).into_response();
        response.extensions_mut().insert(self);
        response
    }
}

/// Source of an error, avoiding boxing for common error types.
#[derive(Debug)]
pub enum ErrorSource {
    Validation(ValidationReport),
    Database(DbError),
    Auth(AuthError),
    Sqlx(sqlx::Error),
    Other(Box<dyn StdError + Send + Sync + 'static>),
}

/// Semantic error categories for consistent error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    BadRequest,
    Unauthorized,
    Forbidden,
    Invalid,     // Validation failures
    Integrity,   // Database constraint violations
    Conflict,    // Business logic conflicts (e.g., version mismatch)
    RateLimited, // API rate limiting
    Unavailable, // Service unavailable
    Other,       // Unexpected errors
}

impl ErrorKind {
    pub fn default_message(&self) -> &'static str {
        match self {
            Self::NotFound => "Resource not found",
            Self::BadRequest => "Bad request",
            Self::Unauthorized => "Authentication required",
            Self::Forbidden => "Permission denied",
            Self::Invalid => "Validation failed",
            Self::Integrity => "Data integrity violation",
            Self::Conflict => "Resource conflict",
            Self::RateLimited => "Too many requests",
            Self::Unavailable => "Service unavailable",
            Self::Other => "Internal server error",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Integrity => StatusCode::CONFLICT,
            Self::Conflict => StatusCode::CONFLICT,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Other => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::BadRequest => "BAD_REQUEST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::Invalid => "VALIDATION_ERROR",
            Self::Integrity => "INTEGRITY_ERROR",
            Self::Conflict => "CONFLICT",
            Self::RateLimited => "RATE_LIMITED",
            Self::Unavailable => "UNAVAILABLE",
            Self::Other => "INTERNAL_ERROR",
        }
    }
}

/// Universal error type for all vyuh handlers (signals, tasks, commands, routes).
/// Provides semantic error categories while preserving full error chains.
/// Context uses SmallVec to avoid heap allocations for typical error chains (0-4 items).
#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub source: Option<ErrorSource>,
    pub context: SmallVec<[Cow<'static, str>; 4]>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind.default_message())?;
        if !self.context.is_empty() {
            write!(f, ": {}", self.context.join(" -> "))?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|src| match src {
            ErrorSource::Auth(e) => e as &(dyn StdError + 'static),
            ErrorSource::Database(e) => e as &(dyn StdError + 'static),
            ErrorSource::Sqlx(e) => e as &(dyn StdError + 'static),
            ErrorSource::Other(e) => e.as_ref() as &(dyn StdError + 'static),
            ErrorSource::Validation(e) => e as &(dyn StdError + 'static),
        })
    }
}

impl Error {
    /// Create error with kind only (use sparingly - prefer wrapping source errors)
    pub fn new(kind: ErrorKind) -> Self {
        Self {
            kind,
            source: None,
            context: SmallVec::new(),
        }
    }

    /// Wrap an external error as ErrorKind::Other (most common case)
    pub fn other<E>(err: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            kind: ErrorKind::Other,
            source: Some(ErrorSource::Other(Box::new(err))),
            context: SmallVec::new(),
        }
    }

    /// Wrap source error with explicit kind (use when semantic categorization matters)
    pub fn wrap<E>(kind: ErrorKind, err: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            kind,
            source: Some(ErrorSource::Other(Box::new(err))),
            context: SmallVec::new(),
        }
    }

    /// Add context to the error chain
    pub fn with_context(mut self, ctx: impl Into<Cow<'static, str>>) -> Self {
        self.context.push(ctx.into());
        self
    }

    /// Get user-facing message
    fn user_message(&self) -> &str {
        self.kind.default_message()
    }

    /// Pretty format for CLI/command output with full error chain
    pub fn display_verbose(&self) -> String {
        let mut output = String::new();

        // Main error message
        output.push_str("Error: ");
        output.push_str(self.kind.default_message());
        output.push('\n');

        // Context chain
        if !self.context.is_empty() {
            for (i, ctx) in self.context.iter().enumerate() {
                output.push_str(&format!(
                    "  {} {}\n",
                    if i == self.context.len() - 1 {
                        "↳"
                    } else {
                        "│"
                    },
                    ctx
                ));
            }
        }

        // Source chain (walk the error chain)
        let mut source_chain: Vec<String> = Vec::new();
        if let Some(src) = &self.source {
            match src {
                ErrorSource::Validation(report) => {
                    if !report.is_empty() {
                        source_chain.push(format!(
                            "Validation failed with {} error(s)",
                            report.issues.len()
                        ));
                    }
                }
                ErrorSource::Database(e) => {
                    source_chain.push(e.to_string());
                }
                ErrorSource::Auth(e) => {
                    source_chain.push(e.to_string());
                }
                ErrorSource::Sqlx(e) => {
                    source_chain.push(e.to_string());
                    // Walk sqlx's source chain
                    let mut current: Option<&(dyn StdError + 'static)> = e.source();
                    while let Some(err) = current {
                        source_chain.push(err.to_string());
                        current = err.source();
                    }
                }
                ErrorSource::Other(e) => {
                    source_chain.push(e.to_string());
                    // Walk generic error's source chain
                    let mut current: Option<&(dyn StdError + 'static)> = e.source();
                    while let Some(err) = current {
                        source_chain.push(err.to_string());
                        current = err.source();
                    }
                }
            }
        }

        if !source_chain.is_empty() {
            output.push('\n');
            output.push_str("Caused by:\n");
            for (i, cause) in source_chain.iter().enumerate() {
                output.push_str(&format!("  {}: {}\n", i, cause));
            }
        }

        output
    }

    /// Compact single-line format for logging
    pub fn display_compact(&self) -> String {
        let mut parts = vec![self.kind.default_message().to_string()];
        if !self.context.is_empty() {
            parts.extend(self.context.iter().map(|c| c.to_string()));
        }
        parts.join(": ")
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        ErrorReport::from(self).into_response()
    }
}

impl From<Error> for ErrorReport {
    fn from(err: Error) -> Self {
        if let Some(ErrorSource::Validation(report)) = err.source {
            return ErrorReport::validation(report);
        }

        let source = match &err.source {
            Some(ErrorSource::Database(_)) | Some(ErrorSource::Sqlx(_)) => {
                ErrorSourceKind::Database
            }
            Some(ErrorSource::Auth(_)) => ErrorSourceKind::Auth,
            Some(ErrorSource::Other(_)) | None => ErrorSourceKind::Application,
            Some(ErrorSource::Validation(_)) => ErrorSourceKind::Validation,
        };

        let mut report = ErrorReport::new(
            err.kind.status_code(),
            source,
            err.kind.error_code().to_ascii_lowercase(),
            err.user_message().to_string(),
        );

        if cfg!(debug_assertions) && !err.context.is_empty() {
            report.errors = Some(serde_json::json!({
                "context": err.context.iter().map(|c| c.to_string()).collect::<Vec<_>>()
            }));
        }

        report
    }
}

impl From<ValidationReport> for ErrorReport {
    fn from(report: ValidationReport) -> Self {
        Self::validation(report)
    }
}

impl From<ValidationReport> for Error {
    fn from(report: ValidationReport) -> Self {
        Self {
            kind: ErrorKind::Invalid,
            source: Some(ErrorSource::Validation(report)),
            context: SmallVec::new(),
        }
    }
}

impl From<ValidationError> for Error {
    fn from(err: ValidationError) -> Self {
        let mut report = ValidationReport::empty();
        report.push_root(err);
        Self::from(report)
    }
}

impl From<DbError> for Error {
    fn from(err: DbError) -> Self {
        let kind = match &err {
            DbError::DoesNotExist => ErrorKind::NotFound,
            DbError::Integrity { .. } => ErrorKind::Integrity,
            DbError::QuerySet(_) => ErrorKind::BadRequest,
            DbError::MultipleObjects => ErrorKind::Conflict,
            _ => ErrorKind::Other,
        };
        Self {
            kind,
            source: Some(ErrorSource::Database(err)),
            context: SmallVec::new(),
        }
    }
}

impl From<AuthError> for Error {
    fn from(err: AuthError) -> Self {
        let kind = match &err {
            AuthError::Forbidden => ErrorKind::Forbidden,
            _ => ErrorKind::Unauthorized,
        };
        Self {
            kind,
            source: Some(ErrorSource::Auth(err)),
            context: SmallVec::new(),
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        let kind = match &err {
            sqlx::Error::RowNotFound => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        };
        Self {
            kind,
            source: Some(ErrorSource::Sqlx(err)),
            context: SmallVec::new(),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::other(err).with_context("JSON parsing failed")
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::other(err).with_context("I/O operation failed")
    }
}

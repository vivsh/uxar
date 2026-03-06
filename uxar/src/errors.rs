use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use smallvec::SmallVec;
use std::{borrow::Cow, error::Error as StdError};
use std::fmt;
use crate::validation::{ValidationError, ValidationReport};
use crate::db::{DbError};
use crate::auth::AuthError;



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
    Invalid,      // Validation failures
    Integrity,    // Database constraint violations
    Conflict,     // Business logic conflicts (e.g., version mismatch)
    RateLimited,  // API rate limiting
    Unavailable,  // Service unavailable
    Other,        // Unexpected errors
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


/// Universal error type for all uxar handlers (signals, tasks, commands, routes).
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
                output.push_str(&format!("  {} {}\n", 
                    if i == self.context.len() - 1 { "↳" } else { "│" },
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
                        source_chain.push(format!("Validation failed with {} error(s)", report.issues.len()));
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
        let status = self.kind.status_code();
        let code = self.kind.error_code();

        // Special handling for validation errors with field-level details
        if let Some(ErrorSource::Validation(report)) = self.source {
            return (status, Json(report.to_nested_map())).into_response();
        }

        // Standard error response
        let mut map = serde_json::Map::new();
        map.insert("detail".to_string(), serde_json::Value::String(self.user_message().to_string()));
        map.insert("code".to_string(), serde_json::Value::String(code.to_string()));

        // Add context in debug mode
        if cfg!(debug_assertions) && !self.context.is_empty() {
            let ctx_array: Vec<_> = self.context
                .iter()
                .map(|c| serde_json::Value::String(c.to_string()))
                .collect();
            map.insert("context".to_string(), serde_json::Value::Array(ctx_array));
        }

        (status, Json(serde_json::Value::Object(map))).into_response()
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
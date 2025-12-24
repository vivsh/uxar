use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::error::Error as StdError;
use std::fmt;
use crate::validation::{ValidationError, ValidationReport};
use crate::db::{DbError, IntegrityKind};
use crate::auth::AuthError;

/// Central error type for API responses.
/// Always returns JSON responses with consistent structure.
#[derive(Debug)]
pub struct ApiError {
    /// HTTP status code
    status: StatusCode,
    /// User-friendly error message
    user_message: String,
    /// Optional error code for programmatic handling
    error_code: Option<String>,
    /// Optional additional details (only shown in debug mode)
    details: Option<String>,
    /// Structured validation errors
    validation_errors: Option<serde_json::Value>,
    /// The underlying error (for logging/debugging)
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl ApiError {
    /// Create a new API error with default status
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            user_message: message.into(),
            error_code: None,
            details: None,
            validation_errors: None,
            source: None,
        }
    }

    /// Set the HTTP status code
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Set an error code for programmatic handling
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }

    /// Add additional details (only shown in debug mode)
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Attach structured validation errors
    pub fn with_validation_errors(mut self, errors: serde_json::Value) -> Self {
        self.validation_errors = Some(errors);
        self
    }

    /// Wrap an existing error
    pub fn wrap<E>(err: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            user_message: "An internal error occurred".to_string(),
            error_code: None,
            details: Some(err.to_string()),
            validation_errors: None,
            source: Some(Box::new(err)),
        }
    }

    // Common error constructors
    pub fn not_found(resource: &str) -> Self {
        Self::new(format!("{} not found", resource))
            .with_status(StatusCode::NOT_FOUND)
            .with_code("NOT_FOUND")
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(message)
            .with_status(StatusCode::BAD_REQUEST)
            .with_code("BAD_REQUEST")
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(message)
            .with_status(StatusCode::UNAUTHORIZED)
            .with_code("UNAUTHORIZED")
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(message)
            .with_status(StatusCode::FORBIDDEN)
            .with_code("FORBIDDEN")
    }

    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::new(message)
            .with_status(StatusCode::UNPROCESSABLE_ENTITY)
            .with_code("VALIDATION_ERROR")
    }

    pub fn internal_error() -> Self {
        Self::new("Internal server error")
            .with_status(StatusCode::INTERNAL_SERVER_ERROR)
            .with_code("INTERNAL_ERROR")
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message)
    }
}

impl StdError for ApiError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &(dyn StdError + 'static))
    }
}

/// JSON response structure for API errors
// Removed intermediate structs to allow dynamic DRF-style responses

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // DRF-style response format
        let body = if let Some(validation_errors) = self.validation_errors {
            // For validation errors, return the map directly (e.g. {"field": ["error"]})
            validation_errors
        } else {
            // For other errors, return {"detail": "message", "code": "..."}
            let mut map = serde_json::Map::new();
            map.insert("detail".to_string(), serde_json::Value::String(self.user_message));
            
            if let Some(code) = self.error_code {
                map.insert("code".to_string(), serde_json::Value::String(code));
            }
            
            // Include debug details if in debug mode
            if cfg!(debug_assertions) {
                if let Some(details) = self.details {
                    map.insert("debug_details".to_string(), serde_json::Value::String(details));
                }
            }
            
            serde_json::Value::Object(map)
        };

        (self.status, Json(body)).into_response()
    }
}

// Generic conversion from anyhow::Error
impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        // anyhow::Error can be converted to Box<dyn StdError>
        let details = format!("{:#}", err);
        let source: Box<dyn StdError + Send + Sync + 'static> = err.into();
        
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            user_message: "An internal error occurred".to_string(),
            error_code: Some("INTERNAL_ERROR".to_string()),
            details: Some(details),
            validation_errors: None,
            source: Some(source),
        }
    }
}

// Generic conversion from Box<dyn Error>
impl From<Box<dyn StdError + Send + Sync + 'static>> for ApiError {
    fn from(err: Box<dyn StdError + Send + Sync + 'static>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            user_message: "An internal error occurred".to_string(),
            error_code: Some("INTERNAL_ERROR".to_string()),
            details: Some(err.to_string()),
            validation_errors: None,
            source: Some(err),
        }
    }
}

// Convenient conversions from common error types
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => ApiError::not_found("Resource"),
            sqlx::Error::Database(db_err) => {
                // Check for common database constraint violations
                let message = db_err.message();
                if message.contains("violates unique constraint") {
                    ApiError::bad_request("Resource already exists")
                } else if message.contains("violates foreign key constraint") {
                    ApiError::bad_request("Invalid reference")
                } else if message.contains("violates check constraint") {
                    ApiError::bad_request("Invalid data")
                } else {
                    ApiError::internal_error().with_details(db_err.message())
                }
            }
            _ => ApiError::internal_error().with_details(err.to_string()),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        ApiError::bad_request("Invalid JSON").with_details(err.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::internal_error().with_details(err.to_string())
    }
}

impl From<ValidationReport> for ApiError {
    fn from(report: ValidationReport) -> Self {
        ApiError::validation_error("Validation failed")
            .with_validation_errors(report.to_nested_map())
    }
}

impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        let mut report = ValidationReport::empty();
        report.push_root(err);
        ApiError::from(report)
    }
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::DoesNotExist => ApiError::not_found("Resource"),
            DbError::Integrity { kind, .. } => match kind {
                IntegrityKind::Unique => ApiError::bad_request("Resource already exists")
                    .with_code("ALREADY_EXISTS"),
                IntegrityKind::ForeignKey => ApiError::bad_request("Invalid reference")
                    .with_code("INVALID_REFERENCE"),
                IntegrityKind::Check => ApiError::bad_request("Invalid data")
                    .with_code("INVALID_DATA"),
                IntegrityKind::NotNull => ApiError::bad_request("Missing required field")
                    .with_code("MISSING_FIELD"),
                _ => ApiError::bad_request("Integrity violation"),
            },
            DbError::Bind(msg) => ApiError::bad_request(format!("Invalid query parameter: {}", msg)),
            DbError::MultipleObjects => ApiError::internal_error()
                .with_details("Query returned multiple rows when one was expected"),
            _ => ApiError::internal_error().with_details(err.to_string()),
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidToken | AuthError::MissingToken | AuthError::ExpiredToken | AuthError::InvalidSignature => {
                ApiError::unauthorized("Invalid authentication credentials")
            }
            AuthError::Forbidden => ApiError::forbidden("Permission denied"),
            AuthError::InternalError(msg) => ApiError::internal_error().with_details(msg),
        }
    }
}


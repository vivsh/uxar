pub(crate) mod select;
pub(crate) mod insert;
pub(crate) mod update;
pub(crate) mod delete;

pub use select::SelectQuery;
pub use insert::InsertQuery;
pub use update::UpdateQuery;
pub use delete::DeleteQuery;

use std::sync::Arc;
use crate::db::argvalue::ArgValue;
use crate::db::commons::{Arguments, Database};

#[derive(Clone, Debug)]
pub struct Statement {
    pub sql: String,
    pub args: Arguments<'static>,
    pub(crate) error: Option<Arc<sqlx::error::BoxDynError>>,
}

impl Statement {
    pub fn new(sql: &str, args: Arguments<'static>) -> Self {
        Self {
            sql: sql.to_string(),
            args,
            error: None,
        }
    }

    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, Database> + sqlx::Type<Database> + Send + 'static,
    {
        use sqlx::Arguments as _;
        match self.args.add(val) {
            Ok(()) => self,
            Err(e) => {
                self.error = Some(Arc::new(e));
                self
            },
        }
    }

    pub fn from_str(sql: &str) -> Self {
        Self {
            sql: sql.to_string(),
            args: Arguments::default(),
            error: None,
        }
    }

    /// Returns the SQL and arguments, or a bind error if one occurred.
    pub fn into_parts(self) -> Result<(String, Arguments<'static>), QueryError> {
        if let Some(err) = self.error {
            return Err(QueryError::BindError(err.to_string()));
        }
        Ok((self.sql, self.args))
    }
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum QueryError {
    #[error("bind error: {0}")]
    BindError(String),
    #[error("source not set")]
    SourceNotSet,
    #[error("placeholder error: {0}")]
    PlaceholderError(#[from] crate::db::placeholders::PlaceholderError),
    #[error("missing binding for {0}")]
    MissingBinding(String),
    #[error("unused binding: {0}")]
    UnusedBinding(String),
    #[error("bind count mismatch: expected {expected}, got {got}")]
    BindCountMismatch { expected: usize, got: usize },
    #[error("invalid identifier '{0}': only alphanumerics, underscores, dots, and spaces are allowed")]
    InvalidIdentifier(String),
}

/// A page of results from a paginated query.
#[derive(Debug, serde::Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

/// Row locking mode for SELECT ... FOR UPDATE / FOR SHARE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Update,
    Share,
}

/// Trait for query builders that support filtering and argument binding.
pub trait FilteredBuilder: Sized {
    fn filter(self, cond: impl Into<std::borrow::Cow<'static, str>>) -> Self;
    fn bind_dyn(self, val: ArgValue) -> Self;
    fn bind_named_dyn(self, name: &str, val: ArgValue) -> Self;
}

/// Validate that a SQL identifier contains only safe characters.
/// Allows alphanumerics, underscores, dots, and spaces (for "table alias" style).
pub(crate) fn validate_ident(s: &str) -> Result<(), QueryError> {
    if s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ' ') {
        Ok(())
    } else {
        Err(QueryError::InvalidIdentifier(s.to_string()))
    }
}

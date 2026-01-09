use crate::db::migrations::actions::Action;

/// Error type for SQL generation failures
#[derive(Debug, thiserror::Error)]
pub enum SqlGenError {
    #[error("unsupported action: {0}")]
    UnsupportedAction(String),
    
    #[error("invalid data: {0}")]
    InvalidData(String),
    
    #[error("missing required field: {0}")]
    MissingField(String),
}

/// Convert migration actions to database-specific SQL statements.
/// A single action may produce multiple SQL statements (e.g., adding a column with a default).
pub trait ActionToSQL {
    /// Generate SQL for a migration action.
    /// Returns a list of SQL statements to execute in order.
    fn action_to_sql(&self, action: &Action) -> Result<Vec<String>, SqlGenError>;
}
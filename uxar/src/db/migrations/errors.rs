
use super::planner::PlannerError;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("store operation failed: {0}")]
    OperationFailed(String),

    #[error("failed to commit migration '{migration_id}': {detail}")]
    CommitFailed {
        migration_id: String,
        detail: String,
    },

    #[error("failed to rollback migration '{migration_id}': {detail}")]
    RollbackFailed {
        migration_id: String,
        detail: String,
    },

    #[error("failed to begin transaction for migration '{migration_id}': {detail}")]
    BeginFailed {
        migration_id: String,
        detail: String,
    },

    #[error("failed to execute action for migration '{migration_id}': {detail}")]
    ActionFailed {
        migration_id: String,
        detail: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("planner error: {0}")]
    Planner(#[from] PlannerError),

    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("failed to replay migration plan: {0}")]
    ReplayError(String),

    #[error("operation failed in migration '{migration_id}': {detail}")]
    OperationFailed {
        migration_id: String,
        detail: String,
    },

    #[error("invalid migration '{0}'")]
    InvalidMigration(String),
}

impl StoreError {
    pub fn operation<E: std::fmt::Display>(e: E) -> Self {
        Self::OperationFailed(e.to_string())
    }

    pub fn commit<E: std::fmt::Display>(migration_id: &str, e: E) -> Self {
        Self::CommitFailed {
            migration_id: migration_id.to_string(),
            detail: e.to_string(),
        }
    }

    pub fn rollback<E: std::fmt::Display>(migration_id: &str, e: E) -> Self {
        Self::RollbackFailed {
            migration_id: migration_id.to_string(),
            detail: e.to_string(),
        }
    }

    pub fn begin<E: std::fmt::Display>(migration_id: &str, e: E) -> Self {
        Self::BeginFailed {
            migration_id: migration_id.to_string(),
            detail: e.to_string(),
        }
    }

    pub fn action<E: std::fmt::Display>(migration_id: &str, e: E) -> Self {
        Self::ActionFailed {
            migration_id: migration_id.to_string(),
            detail: e.to_string(),
        }
    }
}
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FileStorageError {
    #[error("invalid storage name '{name}': {reason}")]
    InvalidName { name: String, reason: String },

    #[error("file storage is not configured")]
    NotConfigured,

    #[error("file storage path escapes storage root: {0}")]
    EscapesRoot(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<FileStorageError> for crate::Error {
    fn from(value: FileStorageError) -> Self {
        crate::Error::other(value)
    }
}

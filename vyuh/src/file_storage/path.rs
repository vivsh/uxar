use std::fmt;
use std::path::{Component, Path, PathBuf};

use super::FileStorageError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageName(String);

impl StorageName {
    pub fn new(name: impl Into<String>) -> Result<Self, FileStorageError> {
        let name = name.into();
        validate_storage_name(&name)?;
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn join_to(&self, root: &Path) -> Result<PathBuf, FileStorageError> {
        let path = root.join(&self.0);
        let normalized_root = root.components().collect::<PathBuf>();
        let normalized_path = path.components().collect::<PathBuf>();
        if !normalized_path.starts_with(&normalized_root) {
            return Err(FileStorageError::EscapesRoot(path));
        }
        Ok(path)
    }
}

impl fmt::Display for StorageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_storage_name(name: &str) -> Result<(), FileStorageError> {
    if name.is_empty() {
        return Err(invalid(name, "cannot be empty"));
    }
    if name.contains('\0') {
        return Err(invalid(name, "cannot contain NUL bytes"));
    }
    if name.contains('\\') {
        return Err(invalid(name, "cannot contain backslashes"));
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return Err(invalid(name, "must be relative"));
    }
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => {}
            Component::CurDir => {}
            Component::ParentDir => return Err(invalid(name, "cannot contain '..'")),
            _ => return Err(invalid(name, "contains invalid path components")),
        }
    }
    Ok(())
}

fn invalid(name: &str, reason: &str) -> FileStorageError {
    FileStorageError::InvalidName {
        name: name.to_string(),
        reason: reason.to_string(),
    }
}

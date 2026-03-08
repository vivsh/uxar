//! Configuration system.
//!
//! Secrets and deployment-specific values go in env vars.
//! Project structure and logic go in source code.

use std::{ffi::OsString, path::PathBuf};

use crate::{auth::AuthConf, db::DbConf, logging, tasks::TaskConf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfError {
    #[error("missing required field '{field}': {reason}")]
    RequiredField { field: String, reason: String },

    #[error("invalid value for '{field}': {reason}{}", expected.as_ref().map(|e| format!(" (expected: {})", e)).unwrap_or_default())]
    InvalidValue {
        field: String,
        reason: String,
        expected: Option<String>,
    },

    #[error("invalid path for '{field}' at '{path}': {reason}")]
    InvalidPath {
        field: String,
        path: String,
        reason: String,
    },

    #[error("validation failed with {} error(s):\n{}", .0.len(), ConfError::display_many(.0))]
    Many(Vec<ConfError>),

    #[error("missing required field: {0}")]
    MissingField(String),

    #[error("{0}")]
    Other(String),
}

impl ConfError{

    fn display_many(errors: &[ConfError]) -> String {
        errors.iter().map(|e| format!("- {}", e)).collect::<Vec<_>>().join("\n")
    }

}

pub fn workspace_root(crate_dir: OsString) -> PathBuf {
    let mut dir = PathBuf::from(crate_dir);

    loop {
        let cargo = dir.join("Cargo.toml");

        if cargo.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo) {
                if content.contains("[workspace]") {
                    return dir;
                }
            }
        }

        if !dir.pop() {
            break;
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn project_dir() -> PathBuf {
    if let Some(crate_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        workspace_root(crate_dir)
    } else {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }
}

fn default_secret_key() -> String {
    format!("dev-secret-{}", env!("CARGO_PKG_NAME"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticDir {
    pub path: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SiteConf {
    pub host: String,

    pub port: u16,

    pub project_dir: String,

    pub database: DbConf,

    #[serde(default = "default_secret_key")]
    pub secret_key: String,

    /// absolute or relative to project_dir
    pub static_dirs: Vec<StaticDir>,

    /// absolute or relative to project_dir
    pub media_dir: Option<String>,

    /// absolute or relative to project_dir
    pub templates_dir: Option<String>,

    /// absolute or relative to project_dir
    pub touch_reload: Option<String>,

    pub log_init: bool,

    pub tz: Option<String>,

    pub auth: AuthConf,

    pub tasks: TaskConf,

    pub logging: logging::LoggingConf,
}

impl Default for SiteConf {
    fn default() -> Self {
        let secret_key = default_secret_key();
        Self {
            host: "localhost".to_string(),
            port: 8080,
            project_dir: project_dir().as_os_str().to_string_lossy().to_string(),
            database: Default::default(),
            secret_key,
            static_dirs: vec![],
            media_dir: None,
            templates_dir: None,
            touch_reload: None,
            log_init: true,
            tz: None,
            auth: AuthConf::default(),
            tasks: TaskConf::default(),
            logging: logging::LoggingConf::default(),
        }
    }
}

impl SiteConf {
    /// Apply env vars as patches. Errors if invalid format.
    pub fn with_env(mut self) -> Result<Self, ConfError> {
        apply_env_patches(&mut self, None)?;
        Ok(self)
    }

    /// Parse from loaded env vars, no validation.
    pub fn from_env() -> Result<Self, ConfError> {
        Self::default().with_env()
    }

    /// Load .env files and parse env vars.
    pub fn from_env_with_files() -> Result<Self, ConfError> {
        Self::load_env_files();
        Self::default().with_env()
    }

    /// Load .env files by build config.
    pub fn load_env_files() {
        dotenvy::dotenv().ok();

        #[cfg(test)]
        dotenvy::from_filename_override(".env.test").ok();

        #[cfg(all(debug_assertions, not(test)))]
        dotenvy::from_filename_override(".env.dev").ok();

        #[cfg(not(any(debug_assertions, test)))]
        dotenvy::from_filename_override(".env.prod").ok();
    }

    /// Load .env from path.
    pub fn load_env_file(path: &str) {
        if let Err(e) = dotenvy::from_filename_override(path) {
            tracing::warn!("Failed to load env file {}: {}", path, e);
        }
    }

    // Chainable setter methods

    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn project_dir(mut self, dir: impl Into<String>) -> Self {
        self.project_dir = dir.into();
        self
    }

    pub fn database(mut self, database: DbConf) -> Self {
        self.database = database;
        self
    }

    pub fn secret_key(mut self, key: impl Into<String>) -> Self {
        self.secret_key = key.into();
        self
    }

    pub fn static_dir(mut self, path: impl Into<String>, url: impl Into<String>) -> Self {
        self.static_dirs.push(StaticDir {
            path: path.into(),
            url: url.into(),
        });
        self
    }

    pub fn media_dir(mut self, dir: impl Into<String>) -> Self {
        self.media_dir = Some(dir.into());
        self
    }

    pub fn templates_dir(mut self, dir: impl Into<String>) -> Self {
        self.templates_dir = Some(dir.into());
        self
    }

    pub fn touch_reload(mut self, path: impl Into<String>) -> Self {
        self.touch_reload = Some(path.into());
        self
    }

    pub fn log_init(mut self, enable: bool) -> Self {
        self.log_init = enable;
        self
    }

    pub fn timezone(mut self, tz: impl Into<String>) -> Self {
        self.tz = Some(tz.into());
        self
    }

    pub fn auth(mut self, auth: AuthConf) -> Self {
        self.auth = auth;
        self
    }

    pub fn tasks(mut self, tasks: TaskConf) -> Self {
        self.tasks = tasks;
        self
    }

    /// Validate config. Returns Ok(()) if valid, or Err(ConfError::Many) with all errors.
    pub fn validate(&self) -> Result<(), ConfError> {
        let mut errors = Vec::new();

        self.validate_required(&mut errors);
        self.validate_database(&mut errors);
        self.validate_paths(&mut errors);

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ConfError::Many(errors))
        }
    }

    fn validate_required(&self, errors: &mut Vec<ConfError>) {
        if self.secret_key.is_empty() {
            errors.push(ConfError::RequiredField {
                field: "secret_key".into(),
                reason: "cannot be empty".into(),
            });
        }
        #[cfg(not(debug_assertions))]
        {
            let default_key = default_secret_key();
            if self.secret_key == default_key {
                errors.push(ConfError::InvalidValue {
                    field: "secret_key".into(),
                    reason: "must not be the default value in release builds".into(),
                    expected: Some("a custom secret key".into()),
                });
            }
        }
        if self.port == 0 {
            errors.push(ConfError::InvalidValue {
                field: "port".into(),
                reason: "must be non-zero".into(),
                expected: Some("1-65535".into()),
            });
        }
        if self.host.is_empty() {
            errors.push(ConfError::RequiredField {
                field: "host".into(),
                reason: "cannot be empty".into(),
            });
        }
    }

    fn validate_database(&self, errors: &mut Vec<ConfError>) {
        #[cfg(not(debug_assertions))]
        {
            if self.database.lazy {
                errors.push(ConfError::InvalidValue {
                    field: "database.lazy".into(),
                    reason: "must be false in release builds".into(),
                    expected: Some("false".into()),
                });
            }
        }
        if self.database.url.is_empty() {
            errors.push(ConfError::RequiredField {
                field: "database.url".into(),
                reason: "cannot be empty".into(),
            });
        }
        if self.database.max_connections == 0 {
            errors.push(ConfError::InvalidValue {
                field: "database.max_connections".into(),
                reason: "must be non-zero".into(),
                expected: Some("positive integer".into()),
            });
        }
        if self.database.min_connections > self.database.max_connections {
            errors.push(ConfError::InvalidValue {
                field: "database.min_connections".into(),
                reason: format!(
                    "cannot exceed max_connections ({} > {})",
                    self.database.min_connections, self.database.max_connections
                ),
                expected: Some(format!("<= {}", self.database.max_connections)),
            });
        }
    }

    fn validate_paths(&self, errors: &mut Vec<ConfError>) {
        let base = PathBuf::from(&self.project_dir);

        validate_dir_readable(&base, "project_dir", errors);

        if let Some(ref dir) = self.media_dir {
            validate_dir_writable(&base, dir, "media_dir", errors);
        }
        if let Some(ref dir) = self.templates_dir {
            validate_dir_readable(&base.join(dir), "templates_dir", errors);
        }
        if let Some(ref file) = self.touch_reload {
            validate_file_writable(&base, file, "touch_reload", errors);
        }
        for (idx, static_dir) in self.static_dirs.iter().enumerate() {
            let field_path = format!("static_dirs[{}].path", idx);
            let field_url = format!("static_dirs[{}].url", idx);

            if static_dir.path.is_empty() {
                errors.push(ConfError::RequiredField {
                    field: field_path.clone(),
                    reason: "cannot be empty".into(),
                });
            } else {
                validate_dir_readable(&base.join(&static_dir.path), &field_path, errors);
            }

            if static_dir.url.is_empty() {
                errors.push(ConfError::RequiredField {
                    field: field_url,
                    reason: "cannot be empty".into(),
                });
            } else if !static_dir.url.starts_with('/') {
                errors.push(ConfError::InvalidValue {
                    field: field_url,
                    reason: "must start with '/'".into(),
                    expected: Some("path starting with '/'".into()),
                });
            }
        }
    }
}

fn apply_env_patches(conf: &mut SiteConf, prefix: Option<&str>) -> Result<(), ConfError> {
    let strip_prefix = |key: &str, pref: Option<&str>| -> String {
        pref.and_then(|p| key.strip_prefix(p))
            .unwrap_or(key)
            .to_lowercase()
    };

    for (key, value) in std::env::vars() {
        if let Some(pref) = prefix {
            if !key.starts_with(pref) {
                continue;
            }
        }

        let field_name = strip_prefix(&key, prefix);

        match field_name.as_str() {
            "database_url" => match DbConf::from_url(&value) {
                Ok(db) => conf.database = db,
                Err(e) => {
                    return Err(ConfError::Other(format!("Database config error: {}", e)));
                }
            },
            "secret_key" => conf.secret_key = value,
            "host" => conf.host = value,
            "port" => match value.parse::<u16>() {
                Ok(p) => conf.port = p,
                Err(_) => {
                    return Err(ConfError::Other(format!(
                        "PORT must be a valid u16, got: {}",
                        value
                    )));
                }
            },
            "tz" => conf.tz = Some(value),
            "log_init" => match value.parse::<bool>() {
                Ok(b) => conf.log_init = b,
                Err(_) => {
                    return Err(ConfError::Other(format!(
                        "LOG_INIT must be 'true' or 'false', got: {}",
                        value
                    )));
                }
            },
            _ => {} // Ignore unknown fields
        }
    }
    Ok(())
}

fn validate_dir_readable(path: &PathBuf, field: &str, errors: &mut Vec<ConfError>) {
    if !path.exists() {
        errors.push(ConfError::InvalidPath {
            field: field.into(),
            path: path.display().to_string(),
            reason: "directory does not exist".into(),
        });
        return;
    }
    if !path.is_dir() {
        errors.push(ConfError::InvalidPath {
            field: field.into(),
            path: path.display().to_string(),
            reason: "not a directory".into(),
        });
        return;
    }
    if let Err(e) = std::fs::read_dir(path) {
        errors.push(ConfError::InvalidPath {
            field: field.into(),
            path: path.display().to_string(),
            reason: format!("cannot read directory: {}", e),
        });
    }
}

fn validate_dir_writable(base: &PathBuf, dir: &str, field: &str, errors: &mut Vec<ConfError>) {
    if dir.is_empty() {
        return;
    }
    let path = base.join(dir);
    validate_dir_readable(&path, field, errors);

    if path.exists() && path.is_dir() {
        let test_file = path.join(format!(".uxar_dir_write_{}", std::process::id()));
        if std::fs::write(&test_file, b"").is_err() {
            errors.push(ConfError::InvalidPath {
                field: field.into(),
                path: path.display().to_string(),
                reason: "directory is not writable".into(),
            });
        } else {
            let _ = std::fs::remove_file(test_file);
        }
    }
}

fn validate_file_writable(base: &PathBuf, file: &str, field: &str, errors: &mut Vec<ConfError>) {
    if file.is_empty() {
        return;
    }
    let path = base.join(file);

    if let Some(parent) = path.parent() {
        if !parent.exists() {
            errors.push(ConfError::InvalidPath {
                field: field.into(),
                path: parent.display().to_string(),
                reason: "parent directory does not exist".into(),
            });
            return;
        }
        if !parent.is_dir() {
            errors.push(ConfError::InvalidPath {
                field: field.into(),
                path: parent.display().to_string(),
                reason: "parent is not a directory".into(),
            });
            return;
        }

        let test_file = parent.join(format!(".uxar_touch_write_{}", std::process::id()));
        if std::fs::write(&test_file, b"").is_err() {
            errors.push(ConfError::InvalidPath {
                field: field.into(),
                path: parent.display().to_string(),
                reason: "parent directory is not writable".into(),
            });
        } else {
            let _ = std::fs::remove_file(test_file);
        }
    }
}

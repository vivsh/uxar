use std::pin::Pin;

use serde::{Deserialize, Serialize};

mod error;
mod local;
mod path;

pub use error::FileStorageError;
pub use local::LocalStorage;
pub use path::StorageName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadConf {
    pub dir: String,
    pub base_url: Option<String>,
    pub temp_dir: Option<String>,
    pub max_request_bytes: u64,
    pub max_file_bytes: u64,
    pub max_files: usize,
    pub max_fields: usize,
    pub memory_threshold_bytes: u64,
}

impl Default for UploadConf {
    fn default() -> Self {
        Self {
            dir: "uploads".into(),
            base_url: Some("/uploads".into()),
            temp_dir: None,
            max_request_bytes: 25 * 1024 * 1024,
            max_file_bytes: 10 * 1024 * 1024,
            max_files: 20,
            max_fields: 100,
            memory_threshold_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SavedFile {
    pub name: StorageName,
    pub path: std::path::PathBuf,
    pub url: Option<String>,
}

impl SavedFile {
    pub fn new(name: StorageName, path: std::path::PathBuf, base_url: Option<String>) -> Self {
        let url = base_url.map(|base| {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                percent_encoding::utf8_percent_encode(
                    name.as_str(),
                    percent_encoding::NON_ALPHANUMERIC
                )
            )
        });
        Self { name, path, url }
    }
}

pub trait StorageBackend: Clone + Send + Sync + 'static {
    fn save<'a>(
        &'a self,
        file: &'a crate::routes::multipart::UploadedFile,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<SavedFile, FileStorageError>> + Send + 'a>>;

    fn save_as<'a>(
        &'a self,
        file: &'a crate::routes::multipart::UploadedFile,
        name: StorageName,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<SavedFile, FileStorageError>> + Send + 'a>>;

    fn open<'a>(
        &'a self,
        name: &'a StorageName,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<tokio::fs::File, FileStorageError>> + Send + 'a,
        >,
    >;

    fn delete<'a>(
        &'a self,
        name: &'a StorageName,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), FileStorageError>> + Send + 'a>>;

    fn url(&self, name: &StorageName) -> Option<String>;
}

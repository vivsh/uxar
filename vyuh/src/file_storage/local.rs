use std::path::{Path, PathBuf};
use std::pin::Pin;

use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::{FileStorageError, SavedFile, StorageBackend, StorageName, UploadConf};
use crate::routes::multipart::UploadedFile;

#[derive(Debug, Clone)]
pub struct LocalStorage {
    root: PathBuf,
    base_url: Option<String>,
}

impl LocalStorage {
    pub fn new(root: impl Into<PathBuf>, base_url: Option<String>) -> Self {
        Self {
            root: root.into(),
            base_url,
        }
    }

    pub fn from_conf(project_dir: &Path, conf: &UploadConf) -> Self {
        Self::new(project_dir.join(&conf.dir), conf.base_url.clone())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn save(&self, file: &UploadedFile) -> Result<SavedFile, FileStorageError> {
        <Self as StorageBackend>::save(self, file).await
    }

    pub async fn save_as(
        &self,
        file: &UploadedFile,
        name: StorageName,
    ) -> Result<SavedFile, FileStorageError> {
        <Self as StorageBackend>::save_as(self, file, name).await
    }

    pub async fn open(&self, name: &StorageName) -> Result<tokio::fs::File, FileStorageError> {
        <Self as StorageBackend>::open(self, name).await
    }

    pub async fn delete(&self, name: &StorageName) -> Result<(), FileStorageError> {
        <Self as StorageBackend>::delete(self, name).await
    }

    pub fn url(&self, name: &StorageName) -> Option<String> {
        <Self as StorageBackend>::url(self, name)
    }

    fn generated_name(&self, file: &UploadedFile) -> Result<StorageName, FileStorageError> {
        let extension = file
            .file_name()
            .and_then(|name| name.rsplit_once('.').map(|(_, ext)| ext))
            .filter(|ext| {
                !ext.is_empty()
                    && ext
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            })
            .map(|ext| format!(".{}", ext.to_ascii_lowercase()))
            .unwrap_or_default();
        StorageName::new(format!("{}{}", Uuid::now_v7(), extension))
    }

    async fn copy_uploaded_file(
        &self,
        file: &UploadedFile,
        name: StorageName,
    ) -> Result<SavedFile, FileStorageError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = name.join_to(&self.root)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if file.is_memory() {
            let mut out = tokio::fs::File::create(&path).await?;
            out.write_all(file.memory_bytes().unwrap_or_default())
                .await?;
            out.flush().await?;
        } else if let Some(temp_path) = file.temp_path() {
            tokio::fs::copy(temp_path, &path).await?;
        } else {
            return Err(FileStorageError::NotConfigured);
        }
        Ok(SavedFile::new(name, path, self.base_url.clone()))
    }
}

impl StorageBackend for LocalStorage {
    fn save<'a>(
        &'a self,
        file: &'a UploadedFile,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<SavedFile, FileStorageError>> + Send + 'a>>
    {
        Box::pin(async move {
            let name = self.generated_name(file)?;
            self.copy_uploaded_file(file, name).await
        })
    }

    fn save_as<'a>(
        &'a self,
        file: &'a UploadedFile,
        name: StorageName,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<SavedFile, FileStorageError>> + Send + 'a>>
    {
        Box::pin(async move { self.copy_uploaded_file(file, name).await })
    }

    fn open<'a>(
        &'a self,
        name: &'a StorageName,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<tokio::fs::File, FileStorageError>> + Send + 'a,
        >,
    > {
        Box::pin(async move { Ok(tokio::fs::File::open(name.join_to(&self.root)?).await?) })
    }

    fn delete<'a>(
        &'a self,
        name: &'a StorageName,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), FileStorageError>> + Send + 'a>> {
        Box::pin(async move {
            let path = name.join_to(&self.root)?;
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err.into()),
            }
        })
    }

    fn url(&self, name: &StorageName) -> Option<String> {
        self.base_url.as_ref().map(|base| {
            format!(
                "{}/{}",
                base.trim_end_matches('/'),
                percent_encoding::utf8_percent_encode(
                    name.as_str(),
                    percent_encoding::NON_ALPHANUMERIC
                )
            )
        })
    }
}

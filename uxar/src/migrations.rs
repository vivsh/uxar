use sqlx::migrate::{Migration, MigrationSource};
use include_dir::Dir;
use std::sync::Arc;
use std::path::PathBuf;
use std::str;
use chrono::{DateTime, Utc};


#[derive(Debug, Clone)]
pub struct EmbeddedMigrationSource {
    dir: Dir<'static>,
}

impl EmbeddedMigrationSource {
    pub fn new(dir: Dir<'static>) -> Self {
        Self { dir }
    }
}

impl MigrationSource<'static> for EmbeddedMigrationSource {
    fn resolve(self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Migration>, Box<dyn std::error::Error + Send + Sync>>> + Send>> {
        Box::pin(async move {
            let mut migrations: Vec<Arc<Migration>> = Vec::new();

        for file in self.dir.files() {
            let filename = file.path().file_name().unwrap().to_str().unwrap();

            if !filename.ends_with(".sql") {
                continue;
            }

            // Example filename: 20230701_120000_init.sql
            let ts = filename.split('_').next().unwrap_or("0");
            let ts = ts.parse::<i64>().unwrap_or(0);
            
            let created_at = DateTime::<Utc>::from_utc(
                chrono::NaiveDateTime::from_timestamp_opt(ts, 0).unwrap_or_default(),
                Utc,
            );

            // let migration = Migration::new(
            //     file.path().to_path_buf(),
            //     str::from_utf8(file.contents())?.to_string(),
            //     created_at,
            // );

            // migrations.push(Arc::new(migration));
        }

            Ok(migrations.into_iter().map(|m| Arc::try_unwrap(m).unwrap_or_else(|arc| (*arc).clone())).collect())
        })
    }
}
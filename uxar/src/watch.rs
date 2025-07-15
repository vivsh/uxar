
use std::path::PathBuf;
use notify::{RecursiveMode, Watcher};
use tokio::signal;
use crate::{SiteError};
use tracing;


pub async fn watch_file(path: PathBuf) -> Result<(), SiteError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<notify::Result<notify::Event>>(1024);
    let mut watcher = notify::recommended_watcher(move |res| {
        tx.try_send(res).ok();
    })
    .map_err(|e| SiteError::ConfigError(format!("Failed to create watcher: {}", e)))?;
    watcher.watch(path.as_path(), RecursiveMode::NonRecursive)
        .map_err(|e| SiteError::ConfigError(format!("Failed to watch file: {}", e)))?;
    rx.recv().await;
    Ok(())
}


async fn reload_signal(touch_reload: Option<String>) -> Result<(), SiteError> {
    if let Some(path) = touch_reload {
        let path = PathBuf::from(path);
        if path.exists() {
            return watch_file(path).await;
        }
    }
    return futures::future::pending().await;
}


async fn interrupt_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
}


pub async fn shutdown_signal(touch_reload: Option<String>) {
    tokio::select! {
        _ = interrupt_signal() => {
            tracing::info!("Ctrl+C received, shutting down gracefully");
        },
        touch = reload_signal(touch_reload) => {
            if let Err(e) = touch {
                tracing::error!("Error during file watch: {}", e);
            }else{
                tracing::info!("File change detected, reloading...");
            }
        },
    }
}

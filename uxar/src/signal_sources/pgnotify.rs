use std::{collections::BTreeMap, sync::Arc};

use futures::future::BoxFuture;
use sqlx::{PgPool, postgres::PgListener};
use tokio::sync::mpsc;

use crate::{
    Site,
    signals::{SignalError, SignalPayload, SignalSource},
};

/// Shared listener that manages a single Postgres LISTEN connection
/// Broadcasts notifications to all registered channels
pub struct PgNotifyListener {
    pool: PgPool,
    channels: BTreeMap<String, mpsc::Sender<Arc<str>>>,
    buffer_size: usize,
}

impl PgNotifyListener {
    pub fn new(pool: PgPool, buffer_size: usize) -> Self {
        Self {
            pool,
            channels: BTreeMap::new(),
            buffer_size,
        }
    }

    /// Subscribe to a channel, returns a receiver for notifications
    pub fn subscribe(&mut self, channel: &str) -> Box<dyn SignalSource> {
        let (tx, rx) = mpsc::channel(self.buffer_size);
        self.channels.insert(channel.to_string(), tx);
        let source = PgNotifySignalSource::new(rx, channel);
        Box::new(source)
    }

    /// Start listening for notifications on all registered channels
    /// Runs until shutdown signal received
    pub async fn run(
        self: Arc<Self>,
        shutdown: Arc<tokio::sync::Notify>,
    ) -> Result<(), SignalError> {
        let mut listener = PgListener::connect_with(&self.pool)
            .await
            .map_err(|e| SignalError::Other(format!("PgListener connect: {}", e).into()))?;

        {
            for channel_name in self.channels.keys() {
                listener.listen(channel_name).await.map_err(|e| {
                    SignalError::Other(format!("LISTEN {}: {}", channel_name, e).into())
                })?;
            }
        }

        loop {
            tokio::select! {
                    _ = shutdown.notified() => {
                        tracing::info!("PgNotifyListener shutting down");
                        break;
                    }
                    msg = listener.recv() => {
                        let notification = msg.map_err(|e| SignalError::Other(format!("Recv error: {}", e).into()))?;
                        let channel_name = notification.channel();
                        let payload: Arc<str> = Arc::from(notification.payload());

                        if let Some(sender) = self.channels.get(channel_name) {
                            if let Err(err) = sender.try_send(payload) {
                                match err {
                                    mpsc::error::TrySendError::Closed(_) => {
                                        tracing::warn!("PgNotify channel '{}' closed, dropping notification", channel_name);
                                    }
                                    mpsc::error::TrySendError::Full(_) => {
                                        tracing::warn!("PgNotify channel '{}' full, dropping notification", channel_name);
                                    }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Signal source that receives Postgres NOTIFY notifications
/// Emits JSON payloads from notification payload
pub struct PgNotifySignalSource {
    pub(crate) channel: String,
    receiver: mpsc::Receiver<Arc<str>>,
}

impl PgNotifySignalSource {
    /// Create a new pg_notify signal source
    ///
    /// # Arguments
    /// * `receiver` - mpsc receiver for notifications
    /// * `channel` - Postgres channel name
    pub fn new(receiver: mpsc::Receiver<Arc<str>>, channel: &str) -> Self {
        Self {
            channel: channel.to_string(),
            receiver,
        }
    }
}

impl crate::signals::SignalSource for PgNotifySignalSource {
    fn poll(&mut self, _site: &Site) -> BoxFuture<'_, Result<Option<SignalPayload>, SignalError>> {
        Box::pin(async move {
            match self.receiver.recv().await {
                Some(payload) => {
                    let json_value: serde_json::Value =
                        serde_json::from_str(&payload).map_err(|e| {
                            SignalError::Other(
                                format!(
                                    "Invalid JSON payload from channel '{}': {}",
                                    self.channel, e
                                )
                                .into(),
                            )
                        })?;
                    Ok(Some(SignalPayload::new(json_value)))
                }
                None => Err(SignalError::Other(
                    format!("PgNotify channel '{}' closed", self.channel).into(),
                )),
            }
        })
    }
}

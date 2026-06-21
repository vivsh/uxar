//! Minimal custom channel backend shape.

use std::{future::Future, sync::Arc};

use vyuh::channels::{
    ChannelBackend, ChannelCursor, ChannelError, ChannelEvent, ChannelEventId, ChannelPublish,
    ChannelReceiver, ChannelTopic, LocalChannelBackend,
};

#[derive(Clone)]
struct AuditedBackend {
    inner: LocalChannelBackend,
}

impl AuditedBackend {
    fn new() -> Self {
        Self {
            inner: LocalChannelBackend::default(),
        }
    }
}

impl ChannelBackend for AuditedBackend {
    fn publish(
        &self,
        event: ChannelPublish,
    ) -> impl Future<Output = Result<ChannelEventId, ChannelError>> + Send {
        let inner = self.inner.clone();
        async move {
            println!("publish {}", event.topic);
            inner.publish(event).await
        }
    }

    fn replay(
        &self,
        topics: &[ChannelTopic],
        after: Option<ChannelCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ChannelEvent>, ChannelError>> + Send {
        let inner = self.inner.clone();
        let topics = topics.to_vec();
        async move { inner.replay(&topics, after, limit).await }
    }

    fn subscribe(
        &self,
        topics: Vec<ChannelTopic>,
    ) -> impl Future<Output = Result<ChannelReceiver, ChannelError>> + Send {
        let inner = self.inner.clone();
        async move { inner.subscribe(topics).await }
    }
}

fn main() {
    let _backend: Arc<dyn Send + Sync> = Arc::new(AuditedBackend::new());
    println!("custom channel backend implemented");
}

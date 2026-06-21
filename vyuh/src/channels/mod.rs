mod backend;
mod transports;
mod types;

pub use backend::{ChannelBackend, ChannelReceiver, LocalChannelBackend};
pub use transports::{ChannelLongPoll, ChannelSse, ChannelWebSocket};
pub use types::{
    ChannelConf, ChannelCursor, ChannelError, ChannelEvent, ChannelEventId, ChannelPublish,
    ChannelSubscription, ChannelTopic, IntoTopics, SlowSubscriberPolicy,
};

use axum::extract::ws::WebSocketUpgrade;

use crate::{
    Data, Error, Site,
    callables::{self, DataValue},
};

#[derive(Clone)]
pub struct ChannelRef {
    backend: LocalChannelBackend,
}

impl ChannelRef {
    pub(crate) fn new(backend: LocalChannelBackend) -> Self {
        Self { backend }
    }

    pub async fn publish<T: DataValue>(
        &self,
        topic: impl TryInto<ChannelTopic, Error = ChannelError>,
        data: Data<T>,
    ) -> Result<ChannelEventId, ChannelError> {
        let value = serde_json::to_value(data.as_ref())
            .map_err(|err| ChannelError::Serialization(err.to_string()))?;
        self.publish_json(topic, value).await
    }

    pub async fn publish_json(
        &self,
        topic: impl TryInto<ChannelTopic, Error = ChannelError>,
        data: serde_json::Value,
    ) -> Result<ChannelEventId, ChannelError> {
        self.backend
            .publish(ChannelPublish {
                topic: topic.try_into()?,
                data,
            })
            .await
    }

    pub async fn sse(&self, topics: impl IntoTopics) -> Result<ChannelSse, ChannelError> {
        self.sse_after(topics, None).await
    }

    pub async fn sse_after(
        &self,
        topics: impl IntoTopics,
        after: Option<ChannelCursor>,
    ) -> Result<ChannelSse, ChannelError> {
        let topics = topics.into_topics()?;
        let replay = self
            .backend
            .replay(&topics, after, self.backend.conf().replay_limit)
            .await?;
        let receiver = self.backend.subscribe(topics).await?;
        Ok(ChannelSse::new(
            replay,
            receiver,
            std::time::Duration::from_millis(self.backend.conf().sse_keepalive_ms),
        ))
    }

    pub async fn websocket(
        &self,
        upgrade: WebSocketUpgrade,
        topics: impl IntoTopics,
    ) -> Result<ChannelWebSocket, ChannelError> {
        self.websocket_after(upgrade, topics, None).await
    }

    pub async fn websocket_after(
        &self,
        upgrade: WebSocketUpgrade,
        topics: impl IntoTopics,
        after: Option<ChannelCursor>,
    ) -> Result<ChannelWebSocket, ChannelError> {
        let topics = topics.into_topics()?;
        let replay = self
            .backend
            .replay(&topics, after, self.backend.conf().replay_limit)
            .await?;
        let receiver = self.backend.subscribe(topics).await?;
        Ok(ChannelWebSocket::new(upgrade, replay, receiver))
    }

    pub async fn long_poll(
        &self,
        topics: impl IntoTopics,
        after: Option<ChannelCursor>,
    ) -> Result<ChannelLongPoll, ChannelError> {
        let topics = topics.into_topics()?;
        let replay = self
            .backend
            .replay(&topics, after, self.backend.conf().replay_limit)
            .await?;
        if !replay.is_empty() {
            return Ok(ChannelLongPoll::from_events(replay));
        }

        let mut receiver = self.backend.subscribe(topics).await?;
        let timeout = std::time::Duration::from_millis(self.backend.conf().long_poll_timeout_ms);
        let event = tokio::time::timeout(timeout, receiver.recv())
            .await
            .ok()
            .flatten();
        Ok(ChannelLongPoll::from_events(
            event
                .map(|event| vec![event.as_ref().clone()])
                .unwrap_or_default(),
        ))
    }
}

impl callables::FromSite for ChannelRef {
    fn from_site(site: &Site) -> Result<Self, callables::CallError> {
        Ok(site.channels())
    }
}

impl axum::extract::FromRequestParts<Site> for ChannelRef {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &Site,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.channels())
    }
}

impl callables::IntoArgPart for ChannelRef {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
}

impl callables::IntoArgPart for WebSocketUpgrade {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
}

impl From<ChannelError> for Error {
    fn from(err: ChannelError) -> Self {
        match err {
            ChannelError::InvalidTopic(_)
            | ChannelError::InvalidCursor(_)
            | ChannelError::TooManyTopics { .. } => Error::bad_request(err.to_string()),
            ChannelError::MessageTooLarge { .. } => Error::bad_request(err.to_string()),
            ChannelError::BackendUnavailable => Error::unavailable(err.to_string()),
            ChannelError::Serialization(_) | ChannelError::Transport(_) => Error::other(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_delivers_to_subscriber() {
        let backend = LocalChannelBackend::default();
        let mut receiver = backend
            .subscribe(vec![ChannelTopic::new("orders.updated").unwrap()])
            .await
            .unwrap();

        backend
            .publish(ChannelPublish {
                topic: ChannelTopic::new("orders.updated").unwrap(),
                data: serde_json::json!({"id": 1}),
            })
            .await
            .unwrap();

        let event = receiver.recv().await.unwrap();
        assert_eq!(event.topic.as_str(), "orders.updated");
    }

    #[tokio::test]
    async fn replay_filters_by_cursor_and_topic() {
        let backend = LocalChannelBackend::default();
        let first = backend
            .publish(ChannelPublish {
                topic: ChannelTopic::new("a").unwrap(),
                data: serde_json::json!(1),
            })
            .await
            .unwrap();
        backend
            .publish(ChannelPublish {
                topic: ChannelTopic::new("b").unwrap(),
                data: serde_json::json!(2),
            })
            .await
            .unwrap();
        backend
            .publish(ChannelPublish {
                topic: ChannelTopic::new("a").unwrap(),
                data: serde_json::json!(3),
            })
            .await
            .unwrap();

        let events = backend
            .replay(
                &[ChannelTopic::new("a").unwrap()],
                Some(ChannelCursor::new(first)),
                10,
            )
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, serde_json::json!(3));
    }

    #[test]
    fn invalid_topic_rejected() {
        assert!(ChannelTopic::new("").is_err());
        assert!(ChannelTopic::new("bad topic").is_err());
        assert!(ChannelTopic::new("bad*topic").is_err());
    }
}

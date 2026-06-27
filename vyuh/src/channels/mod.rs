mod backend;
mod transports;
mod types;

pub use backend::{ChannelBackend, ChannelReceiver, LocalChannelBackend};
pub use transports::{ChannelLongPoll, ChannelResponse, ChannelSse, ChannelWebSocket};
pub use types::{
    ALL_TRANSPORTS, ChannelConf, ChannelCursor, ChannelError, ChannelEvent, ChannelEventId,
    ChannelKey, ChannelTransport, POLL, SSE, SlowSubscriberPolicy, UserKey, WS,
};

use axum::extract::ws::WebSocketUpgrade;

use crate::{
    Error, Site,
    callables::{self, DataValue},
};

/// Site-scoped entry point for signal-backed channel delivery.
///
/// `Channels` does not publish messages directly. Applications emit typed
/// signals through `site.signals().emit(T)`, and channels deliver accepted
/// signal payloads to attached users.
#[derive(Clone)]
pub struct Channels {
    backend: LocalChannelBackend,
}

/// Builder for one user's channel delivery policy.
///
/// The stream describes which signal payload types a user should receive.
/// Attaching a stream replaces the user's previous delivery policy; multiple
/// channel sessions for the same user share that policy and retained queue.
#[derive(Clone)]
pub struct UserStream {
    channels: Channels,
    user_key: UserKey,
    channel_key: Option<ChannelKey>,
    deliveries: Vec<types::DeliverySpec>,
}

pub(crate) struct OpenStream {
    pub(crate) replay: Vec<ChannelEvent>,
    pub(crate) receiver: ChannelReceiver,
    pub(crate) keepalive: std::time::Duration,
    pub(crate) poll_timeout: std::time::Duration,
}

impl Channels {
    pub(crate) fn new(backend: LocalChannelBackend) -> Self {
        Self { backend }
    }

    /// Starts a user-scoped delivery policy builder.
    ///
    /// The returned stream is inert until attached by `Subscriber::attach`.
    pub fn user(&self, user_key: UserKey) -> UserStream {
        UserStream {
            channels: self.clone(),
            user_key,
            channel_key: None,
            deliveries: Vec::new(),
        }
    }

    /// Closes a channel session for a user.
    ///
    /// Returns `true` when a matching live session existed.
    pub async fn close(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> Result<bool, ChannelError> {
        self.backend.close(user_key, channel_key).await
    }

    /// Checks whether a channel session exists for a user.
    pub async fn find(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> Result<bool, ChannelError> {
        self.backend.find(user_key, channel_key).await
    }

    pub(crate) async fn publish_signal<T>(&self, data: &T) -> Result<(), ChannelError>
    where
        T: DataValue,
    {
        self.backend.publish_signal(data).await
    }

    pub(crate) async fn publish_box(
        &self,
        payload: &crate::callables::DataBox,
    ) -> Result<(), ChannelError> {
        self.backend.publish_box(payload).await
    }

    pub(crate) async fn open_stream(
        &self,
        stream: UserStream,
        after: Option<ChannelCursor>,
    ) -> Result<OpenStream, ChannelError> {
        let channel_key = stream.channel_key.unwrap_or_else(ChannelKey::generated);
        let open = self
            .backend
            .open_user(stream.user_key, channel_key, stream.deliveries, after)
            .await?;
        Ok(OpenStream {
            replay: open.replay,
            receiver: open.receiver,
            keepalive: std::time::Duration::from_millis(self.backend.conf().sse_keepalive_ms),
            poll_timeout: std::time::Duration::from_millis(
                self.backend.conf().long_poll_timeout_ms,
            ),
        })
    }
}

impl UserStream {
    /// Assigns an application-owned key to the channel session.
    ///
    /// The key identifies the session/cursor only. It does not filter messages.
    /// If omitted, Vyuh generates a session key for the attachment.
    pub fn channel(mut self, channel_key: ChannelKey) -> Self {
        self.channel_key = Some(channel_key);
        self
    }

    /// Delivers every emitted signal payload of type `T` to this user.
    ///
    /// The payload is retained only after it is accepted by the user policy.
    pub fn deliver<T>(mut self) -> Self
    where
        T: DataValue,
    {
        self.deliveries.push(delivery::<T, _>(|_| true));
        self
    }

    /// Delivers emitted signal payloads of type `T` accepted by `predicate`.
    ///
    /// The predicate runs before serialization and retention. It should be
    /// deterministic and should not perform blocking work.
    pub fn deliver_if<T, F>(mut self, predicate: F) -> Self
    where
        T: DataValue,
        F: Fn(&T) -> bool + Send + Sync + 'static,
    {
        self.deliveries.push(delivery::<T, _>(predicate));
        self
    }

    pub(crate) fn channels(&self) -> Channels {
        self.channels.clone()
    }
}

impl OpenStream {
    pub(crate) fn into_sse(self) -> ChannelSse {
        ChannelSse::new(self.replay, self.receiver, self.keepalive)
    }

    pub(crate) fn into_websocket(self, upgrade: WebSocketUpgrade) -> ChannelWebSocket {
        ChannelWebSocket::new(upgrade, self.replay, self.receiver)
    }

    pub(crate) async fn into_poll(self) -> ChannelLongPoll {
        if !self.replay.is_empty() {
            return ChannelLongPoll::from_events(self.replay);
        }
        let events = ChannelLongPoll::wait(self.receiver, self.poll_timeout).await;
        ChannelLongPoll::from_events(events)
    }
}

fn delivery<T, F>(predicate: F) -> types::DeliverySpec
where
    T: DataValue,
    F: Fn(&T) -> bool + Send + Sync + 'static,
{
    let predicate = std::sync::Arc::new(
        move |value: &dyn std::any::Any| matches!(value.downcast_ref::<T>(), Some(value) if predicate(value)),
    );
    types::DeliverySpec {
        type_id: std::any::TypeId::of::<T>(),
        type_key: short_hash(std::any::type_name::<T>()),
        event_type: event_type::<T>(),
        predicate,
    }
}

fn event_type<T>() -> String
where
    T: DataValue,
{
    <T as schemars::JsonSchema>::schema_name().into_owned()
}

fn short_hash(value: &str) -> String {
    blake3::hash(value.as_bytes())
        .to_hex()
        .chars()
        .take(16)
        .collect()
}

impl callables::FromSite for Channels {
    fn from_site(site: &Site) -> Result<Self, callables::CallError> {
        Ok(site.channels())
    }
}

impl axum::extract::FromRequestParts<Site> for Channels {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        state: &Site,
    ) -> Result<Self, Self::Rejection> {
        Ok(state.channels())
    }
}

impl callables::IntoArgPart for Channels {
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
            ChannelError::InvalidKey(_)
            | ChannelError::InvalidCursor(_)
            | ChannelError::TransportNotAllowed => Error::bad_request(err.to_string()),
            ChannelError::MessageTooLarge { .. } => Error::bad_request(err.to_string()),
            ChannelError::BackendUnavailable => Error::unavailable(err.to_string()),
            ChannelError::Serialization(_) | ChannelError::Transport(_) => Error::other(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    struct TestNotice {
        user_id: i64,
        text: String,
    }

    #[tokio::test]
    async fn user_stream_receives_matching_signal() -> Result<(), Box<dyn std::error::Error>> {
        let channels = Channels::new(LocalChannelBackend::default());
        let stream = channels
            .user(UserKey::new("42")?)
            .deliver_if::<TestNotice, _>(|notice| notice.user_id == 42);
        let mut open = channels.open_stream(stream, None).await?;

        channels
            .publish_signal(&TestNotice {
                user_id: 42,
                text: "ready".to_string(),
            })
            .await?;

        let event =
            match tokio::time::timeout(std::time::Duration::from_millis(100), open.receiver.recv())
                .await?
            {
                Some(event) => event,
                None => return Err("channel closed before event arrived".into()),
            };
        assert_eq!(event.event_type, "TestNotice");
        assert_eq!(event.data["text"], serde_json::json!("ready"));
        Ok(())
    }

    #[tokio::test]
    async fn user_stream_filters_rejected_signal() -> Result<(), Box<dyn std::error::Error>> {
        let channels = Channels::new(LocalChannelBackend::default());
        let stream = channels
            .user(UserKey::new("42")?)
            .deliver_if::<TestNotice, _>(|notice| notice.user_id == 42);
        let mut open = channels.open_stream(stream, None).await?;

        channels
            .publish_signal(&TestNotice {
                user_id: 7,
                text: "hidden".to_string(),
            })
            .await?;

        let event =
            tokio::time::timeout(std::time::Duration::from_millis(25), open.receiver.recv()).await;
        assert!(event.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn repeated_user_policy_replaces_old_rules() -> Result<(), Box<dyn std::error::Error>> {
        let channels = Channels::new(LocalChannelBackend::default());
        let first = channels.user(UserKey::new("42")?).deliver::<TestNotice>();
        let _first_open = channels.open_stream(first, None).await?;

        let second = channels
            .user(UserKey::new("42")?)
            .deliver_if::<TestNotice, _>(|notice| notice.user_id == 7);
        let mut second_open = channels.open_stream(second, None).await?;

        channels
            .publish_signal(&TestNotice {
                user_id: 42,
                text: "old".to_string(),
            })
            .await?;

        let event = tokio::time::timeout(
            std::time::Duration::from_millis(25),
            second_open.receiver.recv(),
        )
        .await;
        assert!(event.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn retained_queue_is_per_user() -> Result<(), Box<dyn std::error::Error>> {
        let channels = Channels::new(LocalChannelBackend::default());
        let stream = channels.user(UserKey::new("42")?).deliver::<TestNotice>();
        let mut first = channels.open_stream(stream, None).await?;

        channels
            .publish_signal(&TestNotice {
                user_id: 42,
                text: "one".to_string(),
            })
            .await?;
        let event = match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            first.receiver.recv(),
        )
        .await?
        {
            Some(event) => event,
            None => return Err("channel closed before event arrived".into()),
        };

        let stream = channels.user(UserKey::new("42")?).deliver::<TestNotice>();
        let second = channels
            .open_stream(stream, Some(ChannelCursor::new(event.id)))
            .await?;

        assert!(second.replay.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn circular_retention_drops_old_messages() -> Result<(), Box<dyn std::error::Error>> {
        let conf = ChannelConf {
            retention_events: 2,
            ..ChannelConf::default()
        };
        let channels = Channels::new(LocalChannelBackend::new(conf));
        let stream = channels.user(UserKey::new("42")?).deliver::<TestNotice>();
        let _open = channels.open_stream(stream, None).await?;

        for text in ["one", "two", "three"] {
            channels
                .publish_signal(&TestNotice {
                    user_id: 42,
                    text: text.to_string(),
                })
                .await?;
        }

        let stream = channels.user(UserKey::new("42")?).deliver::<TestNotice>();
        let open = channels.open_stream(stream, None).await?;
        assert_eq!(open.replay.len(), 2);
        assert_eq!(
            open.replay.first().map(|event| &event.data["text"]),
            Some(&serde_json::json!("two"))
        );
        Ok(())
    }
}

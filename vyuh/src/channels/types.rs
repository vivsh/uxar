use std::{fmt, str::FromStr, time::SystemTime};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelConf {
    pub enabled: bool,
    pub subscriber_queue: usize,
    pub command_queue: usize,
    pub replay_limit: usize,
    pub retention_events: usize,
    pub max_topics_per_subscribe: usize,
    pub max_topic_len: usize,
    pub max_message_bytes: usize,
    pub long_poll_timeout_ms: u64,
    pub sse_keepalive_ms: u64,
    pub slow_subscriber_policy: SlowSubscriberPolicy,
}

impl Default for ChannelConf {
    fn default() -> Self {
        Self {
            enabled: true,
            subscriber_queue: 256,
            command_queue: 1024,
            replay_limit: 256,
            retention_events: 10_000,
            max_topics_per_subscribe: 64,
            max_topic_len: 256,
            max_message_bytes: 1024 * 1024,
            long_poll_timeout_ms: 25_000,
            sse_keepalive_ms: 15_000,
            slow_subscriber_policy: SlowSubscriberPolicy::Disconnect,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlowSubscriberPolicy {
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ChannelTopic(String);

impl ChannelTopic {
    pub fn new(topic: impl Into<String>) -> Result<Self, ChannelError> {
        Self::new_with_limit(topic, ChannelConf::default().max_topic_len)
    }

    pub(crate) fn new_with_limit(
        topic: impl Into<String>,
        max_len: usize,
    ) -> Result<Self, ChannelError> {
        let topic = topic.into();
        if topic.is_empty() {
            return Err(ChannelError::InvalidTopic("topic cannot be empty".into()));
        }
        if topic.len() > max_len {
            return Err(ChannelError::InvalidTopic(format!(
                "topic cannot exceed {max_len} bytes"
            )));
        }
        if topic.contains('*') {
            return Err(ChannelError::InvalidTopic(
                "wildcard topics are not supported".into(),
            ));
        }
        if topic
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | ':' | '/')))
        {
            return Err(ChannelError::InvalidTopic(
                "topic may contain only alphanumeric characters, '.', '_', '-', ':', and '/'"
                    .into(),
            ));
        }
        Ok(Self(topic))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for ChannelTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ChannelTopic {
    type Err = ChannelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for ChannelTopic {
    type Error = ChannelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for ChannelTopic {
    type Error = ChannelError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ChannelEventId(u64);

impl ChannelEventId {
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ChannelEventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelCursor(ChannelEventId);

impl ChannelCursor {
    pub fn new(id: ChannelEventId) -> Self {
        Self(id)
    }

    pub fn event_id(self) -> ChannelEventId {
        self.0
    }
}

impl fmt::Display for ChannelCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for ChannelCursor {
    type Err = ChannelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = value
            .parse::<u64>()
            .map_err(|_| ChannelError::InvalidCursor(value.to_string()))?;
        Ok(Self(ChannelEventId(id)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelEvent {
    pub id: ChannelEventId,
    pub topic: ChannelTopic,
    pub data: serde_json::Value,
    pub created_at: u64,
}

impl ChannelEvent {
    pub(crate) fn new(id: ChannelEventId, topic: ChannelTopic, data: serde_json::Value) -> Self {
        Self {
            id,
            topic,
            data,
            created_at: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelPublish {
    pub topic: ChannelTopic,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelSubscription {
    pub topics: Vec<ChannelTopic>,
}

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("invalid channel topic: {0}")]
    InvalidTopic(String),

    #[error("invalid channel cursor: {0}")]
    InvalidCursor(String),

    #[error("too many channel topics: maximum is {max}, got {got}")]
    TooManyTopics { max: usize, got: usize },

    #[error("channel message is too large: maximum is {max} bytes, got {got}")]
    MessageTooLarge { max: usize, got: usize },

    #[error("channel backend unavailable")]
    BackendUnavailable,

    #[error("channel serialization failed: {0}")]
    Serialization(String),

    #[error("channel transport failed: {0}")]
    Transport(String),
}

pub trait IntoTopics {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError>;
}

impl IntoTopics for Vec<ChannelTopic> {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        Ok(self)
    }
}

impl IntoTopics for Vec<String> {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        self.into_iter().map(ChannelTopic::new).collect()
    }
}

impl IntoTopics for Vec<&str> {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        self.into_iter().map(ChannelTopic::new).collect()
    }
}

impl<const N: usize> IntoTopics for [ChannelTopic; N] {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        Ok(self.into_iter().collect())
    }
}

impl IntoTopics for ChannelTopic {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        Ok(vec![self])
    }
}

impl IntoTopics for &str {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        Ok(vec![ChannelTopic::new(self)?])
    }
}

impl IntoTopics for String {
    fn into_topics(self) -> Result<Vec<ChannelTopic>, ChannelError> {
        Ok(vec![ChannelTopic::new(self)?])
    }
}

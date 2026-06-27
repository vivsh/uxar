use std::{
    any::TypeId,
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use parking_lot::Mutex;
use tokio::sync::mpsc;

use super::types::{DeliveryRule, DeliverySpec};
use super::{
    ChannelConf, ChannelCursor, ChannelError, ChannelEvent, ChannelEventId, ChannelKey, UserKey,
};

/// Storage and fanout contract for channel delivery.
///
/// Backends own user policies, per-user retained queues, channel cursor
/// sessions, replay, and live wakeup. Application code should normally use
/// `Channels` instead of this trait.
pub trait ChannelBackend: Send + Sync + 'static {
    /// Atomically registers the user's full delivery policy and opens a channel.
    ///
    /// Re-registering the same user replaces older rules. The returned replay
    /// and live receiver must be opened without a gap where accepted messages
    /// can be missed.
    fn open_user(
        &self,
        user_key: UserKey,
        channel_key: ChannelKey,
        deliveries: Vec<DeliverySpec>,
        after: Option<ChannelCursor>,
    ) -> impl Future<Output = Result<ChannelOpen, ChannelError>> + Send;

    /// Offers a typed signal payload to users whose policy accepts that type.
    ///
    /// Implementations should run predicates before serialization and should
    /// return `Ok(())` when no user accepts the payload.
    fn publish_signal<T>(&self, value: &T) -> impl Future<Output = Result<(), ChannelError>> + Send
    where
        T: crate::callables::DataValue;

    /// Offers a type-erased but serializable signal payload to channel users.
    ///
    /// This exists so emitter-produced `Data<T>` can follow the same channel
    /// path after it has been erased by callable dispatch.
    fn publish_box(
        &self,
        payload: &crate::callables::DataBox,
    ) -> impl Future<Output = Result<(), ChannelError>> + Send;

    /// Closes one channel session for a user.
    ///
    /// Returns `true` when a live channel was found and removed.
    fn close(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> impl Future<Output = Result<bool, ChannelError>> + Send;

    /// Checks whether a channel session currently exists for a user.
    fn find(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> impl Future<Output = Result<bool, ChannelError>> + Send;
}

/// Live receiver for accepted channel events.
#[derive(Debug)]
pub struct ChannelReceiver {
    pub(crate) inner: mpsc::Receiver<Arc<ChannelEvent>>,
}

impl ChannelReceiver {
    fn new(inner: mpsc::Receiver<Arc<ChannelEvent>>) -> Self {
        Self { inner }
    }

    /// Receives the next live channel event, or `None` after the channel closes.
    pub async fn recv(&mut self) -> Option<Arc<ChannelEvent>> {
        self.inner.recv().await
    }
}

/// Result of atomically opening a channel session.
///
/// `replay` contains retained events after the requested cursor; `receiver`
/// carries subsequent live events.
pub struct ChannelOpen {
    pub replay: Vec<ChannelEvent>,
    pub receiver: ChannelReceiver,
}

/// In-memory channel backend used by the default site runtime.
///
/// This backend is process-local. It is optimized for low ceremony and fast
/// in-process fanout, not durability or cross-process delivery.
#[derive(Clone)]
pub struct LocalChannelBackend {
    inner: Arc<LocalChannelInner>,
}

struct LocalChannelInner {
    conf: ChannelConf,
    next_id: AtomicU64,
    state: Mutex<LocalChannelState>,
}

#[derive(Default)]
struct LocalChannelState {
    users: HashMap<UserKey, UserState>,
    type_index: HashMap<TypeId, HashSet<UserKey>>,
}

#[derive(Default)]
struct UserState {
    rules: HashMap<TypeId, DeliveryRule>,
    queue: VecDeque<Arc<ChannelEvent>>,
    channels: HashMap<ChannelKey, Subscriber>,
}

struct Subscriber {
    sender: mpsc::Sender<Arc<ChannelEvent>>,
}

impl LocalChannelBackend {
    /// Creates an in-memory backend using the provided channel configuration.
    pub fn new(conf: ChannelConf) -> Self {
        Self {
            inner: Arc::new(LocalChannelInner {
                conf,
                next_id: AtomicU64::new(1),
                state: Mutex::new(LocalChannelState::default()),
            }),
        }
    }

    /// Returns the backend configuration.
    pub fn conf(&self) -> &ChannelConf {
        &self.inner.conf
    }

    fn validate_payload_size(&self, value: &serde_json::Value) -> Result<(), ChannelError> {
        let size = serde_json::to_vec(value)
            .map_err(|err| ChannelError::Serialization(err.to_string()))?
            .len();
        if size > self.inner.conf.max_message_bytes {
            return Err(ChannelError::MessageTooLarge {
                max: self.inner.conf.max_message_bytes,
                got: size,
            });
        }
        Ok(())
    }

    fn accepted_users<T>(&self, value: &T) -> Vec<UserKey>
    where
        T: crate::callables::DataValue,
    {
        let type_id = TypeId::of::<T>();
        let any = value as &dyn std::any::Any;
        self.accepted_users_any(type_id, any)
    }

    fn accepted_users_any(&self, type_id: TypeId, value: &dyn std::any::Any) -> Vec<UserKey> {
        let state = self.inner.state.lock();
        let Some(users) = state.type_index.get(&type_id) else {
            return Vec::new();
        };
        users
            .iter()
            .filter(|user_key| user_accepts(&state, user_key, type_id, value))
            .cloned()
            .collect()
    }

    fn append_event(
        &self,
        users: &[UserKey],
        type_id: TypeId,
        value: &dyn std::any::Any,
        event: Arc<ChannelEvent>,
    ) {
        let mut state = self.inner.state.lock();
        for user_key in users {
            if !user_accepts(&state, user_key, type_id, value) {
                continue;
            }
            let Some(user) = state.users.get_mut(user_key) else {
                continue;
            };
            retain_event(user, Arc::clone(&event), self.inner.conf.retention_events);
            send_live(user, Arc::clone(&event));
        }
    }

    fn replace_policy(
        state: &mut LocalChannelState,
        user_key: &UserKey,
        deliveries: Vec<DeliverySpec>,
    ) {
        remove_indexes(state, user_key);
        let user = state.users.entry(user_key.clone()).or_default();
        user.rules.clear();
        for delivery in deliveries {
            state
                .type_index
                .entry(delivery.type_id)
                .or_default()
                .insert(user_key.clone());
            user.rules.insert(
                delivery.type_id,
                DeliveryRule {
                    predicate: delivery.predicate,
                },
            );
        }
    }
}

impl Default for LocalChannelBackend {
    fn default() -> Self {
        Self::new(ChannelConf::default())
    }
}

impl ChannelBackend for LocalChannelBackend {
    async fn open_user(
        &self,
        user_key: UserKey,
        channel_key: ChannelKey,
        deliveries: Vec<DeliverySpec>,
        after: Option<ChannelCursor>,
    ) -> Result<ChannelOpen, ChannelError> {
        let (sender, receiver) = mpsc::channel(self.inner.conf.subscriber_queue.max(1));
        let after = after.map(|cursor| cursor.event_id().as_u64()).unwrap_or(0);
        let mut state = self.inner.state.lock();
        Self::replace_policy(&mut state, &user_key, deliveries);
        let user = state.users.entry(user_key).or_default();
        let replay = replay(user, after, self.inner.conf.replay_limit);
        user.channels.insert(channel_key, Subscriber { sender });
        Ok(ChannelOpen {
            replay,
            receiver: ChannelReceiver::new(receiver),
        })
    }

    async fn publish_signal<T>(&self, value: &T) -> Result<(), ChannelError>
    where
        T: crate::callables::DataValue,
    {
        let type_id = TypeId::of::<T>();
        let accepted = self.accepted_users(value);
        if accepted.is_empty() {
            return Ok(());
        }
        let data = serde_json::to_value(value)
            .map_err(|err| ChannelError::Serialization(err.to_string()))?;
        self.validate_payload_size(&data)?;
        let event_type = event_type::<T>();
        let id = ChannelEventId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let event = Arc::new(ChannelEvent::new(id, event_type, data));
        self.append_event(&accepted, type_id, value, event);
        Ok(())
    }

    async fn publish_box(&self, payload: &crate::callables::DataBox) -> Result<(), ChannelError> {
        let type_id = payload.payload_type_id();
        let value = payload.as_any();
        let accepted = self.accepted_users_any(type_id, value);
        if accepted.is_empty() {
            return Ok(());
        }
        let data = payload
            .to_json()
            .ok_or_else(|| ChannelError::Serialization("payload is not serializable".into()))?
            .map_err(ChannelError::Serialization)?;
        self.validate_payload_size(&data)?;
        let event_type = payload
            .schema_name()
            .map(|name| sanitize_event_type(&name))
            .ok_or_else(|| ChannelError::Serialization("payload schema is not available".into()))?;
        let id = ChannelEventId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let event = Arc::new(ChannelEvent::new(id, event_type, data));
        self.append_event(&accepted, type_id, value, event);
        Ok(())
    }

    async fn close(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> Result<bool, ChannelError> {
        let mut state = self.inner.state.lock();
        let Some(user) = state.users.get_mut(user_key) else {
            return Ok(false);
        };
        Ok(user.channels.remove(channel_key).is_some())
    }

    async fn find(
        &self,
        user_key: &UserKey,
        channel_key: &ChannelKey,
    ) -> Result<bool, ChannelError> {
        let state = self.inner.state.lock();
        Ok(state
            .users
            .get(user_key)
            .map(|user| user.channels.contains_key(channel_key))
            .unwrap_or(false))
    }
}

fn remove_indexes(state: &mut LocalChannelState, user_key: &UserKey) {
    for users in state.type_index.values_mut() {
        users.remove(user_key);
    }
    state.type_index.retain(|_, users| !users.is_empty());
}

fn user_accepts(
    state: &LocalChannelState,
    user_key: &UserKey,
    type_id: TypeId,
    value: &dyn std::any::Any,
) -> bool {
    state
        .users
        .get(user_key)
        .and_then(|user| user.rules.get(&type_id))
        .map(|rule| (rule.predicate)(value))
        .unwrap_or(false)
}

fn retain_event(user: &mut UserState, event: Arc<ChannelEvent>, retention_events: usize) {
    if retention_events == 0 {
        return;
    }
    user.queue.push_back(event);
    while user.queue.len() > retention_events {
        user.queue.pop_front();
    }
}

fn send_live(user: &mut UserState, event: Arc<ChannelEvent>) {
    let mut closed = Vec::new();
    for (key, subscriber) in user.channels.iter() {
        match subscriber.sender.try_send(Arc::clone(&event)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) | Err(mpsc::error::TrySendError::Closed(_)) => {
                closed.push(key.clone())
            }
        }
    }
    for key in closed {
        user.channels.remove(&key);
    }
}

fn replay(user: &UserState, after: u64, limit: usize) -> Vec<ChannelEvent> {
    user.queue
        .iter()
        .filter(|event| event.id.as_u64() > after)
        .take(limit)
        .map(|event| event.as_ref().clone())
        .collect()
}

fn event_type<T>() -> String
where
    T: crate::callables::DataValue,
{
    sanitize_event_type(<T as schemars::JsonSchema>::schema_name().as_ref())
}

fn sanitize_event_type(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\r' | '\n' => '_',
            _ => ch,
        })
        .collect()
}

use std::{
    collections::{HashMap, HashSet, VecDeque},
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use parking_lot::Mutex;
use tokio::sync::{broadcast, mpsc};

use super::{
    ChannelConf, ChannelCursor, ChannelError, ChannelEvent, ChannelEventId, ChannelPublish,
    ChannelTopic,
};

pub trait ChannelBackend: Send + Sync + 'static {
    fn publish(
        &self,
        event: ChannelPublish,
    ) -> impl Future<Output = Result<ChannelEventId, ChannelError>> + Send;

    fn replay(
        &self,
        topics: &[ChannelTopic],
        after: Option<ChannelCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ChannelEvent>, ChannelError>> + Send;

    fn subscribe(
        &self,
        topics: Vec<ChannelTopic>,
    ) -> impl Future<Output = Result<ChannelReceiver, ChannelError>> + Send;
}

#[derive(Debug)]
pub struct ChannelReceiver {
    pub(crate) inner: mpsc::Receiver<Arc<ChannelEvent>>,
}

impl ChannelReceiver {
    fn new(inner: mpsc::Receiver<Arc<ChannelEvent>>) -> Self {
        Self { inner }
    }

    pub async fn recv(&mut self) -> Option<Arc<ChannelEvent>> {
        self.inner.recv().await
    }
}

#[derive(Clone)]
pub struct LocalChannelBackend {
    inner: Arc<LocalChannelInner>,
}

struct LocalChannelInner {
    conf: ChannelConf,
    next_id: AtomicU64,
    state: Mutex<LocalChannelState>,
    wake: broadcast::Sender<()>,
}

#[derive(Default)]
struct LocalChannelState {
    retention: VecDeque<Arc<ChannelEvent>>,
    topic_map: HashMap<ChannelTopic, Vec<Subscriber>>,
}

struct Subscriber {
    id: uuid::Uuid,
    sender: mpsc::Sender<Arc<ChannelEvent>>,
}

impl LocalChannelBackend {
    pub fn new(conf: ChannelConf) -> Self {
        let (wake, _) = broadcast::channel(conf.command_queue.max(16));
        Self {
            inner: Arc::new(LocalChannelInner {
                conf,
                next_id: AtomicU64::new(1),
                state: Mutex::new(LocalChannelState::default()),
                wake,
            }),
        }
    }

    pub fn conf(&self) -> &ChannelConf {
        &self.inner.conf
    }

    fn validate_topics(
        &self,
        topics: Vec<ChannelTopic>,
    ) -> Result<Vec<ChannelTopic>, ChannelError> {
        if topics.len() > self.inner.conf.max_topics_per_subscribe {
            return Err(ChannelError::TooManyTopics {
                max: self.inner.conf.max_topics_per_subscribe,
                got: topics.len(),
            });
        }
        let mut seen = HashSet::with_capacity(topics.len());
        let mut unique = Vec::with_capacity(topics.len());
        for topic in topics {
            let topic =
                ChannelTopic::new_with_limit(topic.into_string(), self.inner.conf.max_topic_len)?;
            if seen.insert(topic.clone()) {
                unique.push(topic);
            }
        }
        Ok(unique)
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
}

impl Default for LocalChannelBackend {
    fn default() -> Self {
        Self::new(ChannelConf::default())
    }
}

impl ChannelBackend for LocalChannelBackend {
    async fn publish(&self, event: ChannelPublish) -> Result<ChannelEventId, ChannelError> {
        self.validate_payload_size(&event.data)?;
        let topic =
            ChannelTopic::new_with_limit(event.topic.into_string(), self.inner.conf.max_topic_len)?;
        let id = ChannelEventId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let event = Arc::new(ChannelEvent::new(id, topic.clone(), event.data));
        let mut closed = Vec::new();

        {
            let mut state = self.inner.state.lock();
            state.retention.push_back(Arc::clone(&event));
            while state.retention.len() > self.inner.conf.retention_events {
                state.retention.pop_front();
            }

            if let Some(subscribers) = state.topic_map.get_mut(&topic) {
                for subscriber in subscribers.iter() {
                    match subscriber.sender.try_send(Arc::clone(&event)) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_))
                        | Err(mpsc::error::TrySendError::Closed(_)) => {
                            closed.push(subscriber.id);
                        }
                    }
                }
                if !closed.is_empty() {
                    subscribers.retain(|sub| !closed.contains(&sub.id));
                }
            }
        }

        let _ = self.inner.wake.send(());
        Ok(id)
    }

    async fn replay(
        &self,
        topics: &[ChannelTopic],
        after: Option<ChannelCursor>,
        limit: usize,
    ) -> Result<Vec<ChannelEvent>, ChannelError> {
        let topics = self.validate_topics(topics.to_vec())?;
        let topic_set: HashSet<ChannelTopic> = topics.into_iter().collect();
        let after = after.map(|cursor| cursor.event_id().as_u64()).unwrap_or(0);
        let limit = limit.min(self.inner.conf.replay_limit);

        let state = self.inner.state.lock();
        Ok(state
            .retention
            .iter()
            .filter(|event| event.id.as_u64() > after && topic_set.contains(&event.topic))
            .take(limit)
            .map(|event| event.as_ref().clone())
            .collect())
    }

    async fn subscribe(&self, topics: Vec<ChannelTopic>) -> Result<ChannelReceiver, ChannelError> {
        let topics = self.validate_topics(topics)?;
        let (sender, receiver) = mpsc::channel(self.inner.conf.subscriber_queue.max(1));
        let subscriber_id = uuid::Uuid::now_v7();

        let mut state = self.inner.state.lock();
        for topic in topics {
            state.topic_map.entry(topic).or_default().push(Subscriber {
                id: subscriber_id,
                sender: sender.clone(),
            });
        }

        Ok(ChannelReceiver::new(receiver))
    }
}

use std::{
    borrow::Cow, collections::HashMap, convert::Infallible, sync::Arc
};
use axum::{http::response, response::sse::{Event, KeepAlive, Sse}};
use futures::stream::{Stream, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

/// Compact topic identifier using u32 instead of string for fast lookups.
/// Topics are interned at the boundary (publish/subscribe) and this ID is used
/// throughout the system to avoid string hashing and cloning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TopicId(u32);

use crate::{Site, callables, signals};

/// Errors that can occur during channel operations.
pub enum ChannelError {
    SubscribeError,
    PublishError,
    UnsubscribeError,
}

/// Different ways to unsubscribe: by individual subscription, by user, or by channel.
#[derive(Debug)]
pub enum Unsubscribe {
    User(Cow<'static, str>),
    Channel(Cow<'static, str>),
    Subscription(uuid::Uuid),
}

/// Handle returned to subscribers containing their unique ID and webhook URL.
#[derive(Debug)]
pub struct ChannelSubscription {
    pub id: uuid::Uuid,
    pub url: String,
}

pub trait ChannelService {
    fn send(
        &self,
        req: ChannelRequest,
    ) -> impl Future<Output = Result<(), ChannelError>>;
}

/// Messages sent to the channel actor. These are processed in a single-threaded
/// event loop, avoiding locks and ensuring consistent ordering.
/// Internal to the channel service - external users call methods on LocalChannelService.
#[derive(Debug)]
pub enum ChannelMessage {
    /// Publish an event to a topic. Topic string is interned at the boundary.
    Publish {
        topic: String,
        message: String,
        user_keys: Option<Vec<String>>,
    },
    /// Subscribe to multiple topics. Returns a receiver for events.
    Subscribe {
        user_key: String,
        channel_key: String,
        topics: Vec<String>,
    },
    /// Unsubscribe using one of three strategies.
    Unsubscribe {
        unsub: Unsubscribe,
    },
}

/// Responses sent back from the channel actor.
/// Internal to the channel service.
#[derive(Debug)]
pub enum ChannelResponse {
    Publish(usize),
    Subscription((ChannelSubscription, mpsc::Receiver<Arc<ChannelEvent>>)),
    Unsubscribe(usize),
}

/// Event sent to subscribers. Contains only the message payload.
/// Subscribers already know which topics they're listening to, so topic ID is internal only.
#[derive(Debug)]
pub struct ChannelEvent {
    topic_id: TopicId,
    user_keys: Option<Vec<String>>,
    pub topic: String,
    pub message: String,
}

impl ChannelEvent {

}

/// Request envelope containing a message and reply channel for responses.
/// Internal actor protocol.
#[derive(Debug)]
pub struct ChannelRequest {
    pub message: ChannelMessage,
    pub reply: oneshot::Sender<ChannelResponse>,
}

/// Internal subscription state. Stores reverse edges (topics) for efficient cleanup,
/// plus user/channel keys for bulk unsubscribe operations.
#[derive(Debug)]
struct LocalSubscription {
    id: uuid::Uuid,
    user_key: String,
    channel_key: String,
    /// Topic IDs (not strings) for O(1) cleanup when unsubscribing.
    topics: Vec<TopicId>,
    sender: mpsc::Sender<Arc<ChannelEvent>>,
}

/// Public interface to the channel service. Sends requests to the actor
/// via an mpsc channel for single-threaded processing.
pub struct LocalChannelService {
    queue_size: usize,
    base_url: String,
    sender: mpsc::Sender<ChannelRequest>,
}

impl ChannelService for LocalChannelService {

    async fn send(
        &self,
        req: ChannelRequest,
    ) -> Result<(), ChannelError> {
        self.sender
            .send(req)
            .await
            .map_err(|_| ChannelError::PublishError)?;
        Ok(())        
    }
}

impl LocalChannelService {
    pub fn new(sender: mpsc::Sender<ChannelRequest>,  base_url: String, queue_size: usize) -> Self {
        Self {
            queue_size,
            base_url,
            sender,
        }
    }

    pub fn make_absolute_url(&self, sub_id: uuid::Uuid, channel_key: &str) -> String {
        format!(
            "{}{}",
            self.base_url,
            format!("/channels/{}/subscriptions/{}", channel_key, sub_id)
        )
    }

    /// Process a single request from the channel. Called sequentially in the event loop.
    /// Interning happens here at the boundary for publish operations.
    fn handle_request(
        &self,
        inner: &mut ChannelInner,
        request: ChannelRequest,
    ) -> Result<(), ChannelResponse> {
        match request.message {
            ChannelMessage::Publish { topic, message, user_keys } => {
                // Intern topic string → TopicId at the boundary
                let delivered = inner.publish(topic, message, user_keys);
                request.reply.send(ChannelResponse::Publish(delivered))
            }
            ChannelMessage::Subscribe {
                user_key,
                channel_key,
                topics,
            } => {
                let (sub_id, receiver) =
                    inner.subscribe(user_key.clone(), channel_key.clone(), topics);
                let subscription = ChannelSubscription {
                    id: sub_id,
                    url: self.make_absolute_url(sub_id, &channel_key),
                };
                request
                    .reply
                    .send(ChannelResponse::Subscription((subscription, receiver)))
            }
            ChannelMessage::Unsubscribe { unsub } => match unsub {
                Unsubscribe::Subscription(sub_id) => {
                    let success = inner.unsubscribe(sub_id);
                    request.reply.send(ChannelResponse::Unsubscribe(success))
                }
                Unsubscribe::User(user_key) => {
                    let count = inner.unsubscribe_by_user(&user_key);
                    request.reply.send(ChannelResponse::Unsubscribe(count))
                }
                Unsubscribe::Channel(channel_key) => {
                    let count = inner.unsubscribe_by_channel(&channel_key);
                    request.reply.send(ChannelResponse::Unsubscribe(count))
                }
            },
        }
    }

    /// Main event loop. Processes requests in batches to amortize channel overhead.
    /// Single-threaded: all state mutations happen sequentially, no locks needed.
    pub async fn run(self: Arc<Self>, mut rx: mpsc::Receiver<ChannelRequest>, site: Site) {
        let capacity = self.queue_size;
        let mut inner = ChannelInner::new(capacity);
        let shutdown = site.shutdown_notifier();
        // Pre-allocated buffer for drain-batch optimization (up to 32 requests)
        let mut batch = Vec::with_capacity(32);

        loop {
            tokio::select! {
                Some(request) = rx.recv() => {
                    // Drain-batch: pull as many pending requests as possible
                    // to amortize the cost of the async channel recv
                    batch.push(request);
                    while let Ok(req) = rx.try_recv() {
                        batch.push(req);
                        if batch.len() >= 32 {
                            break;
                        }
                    }
                    // Process all requests in the batch sequentially
                    for request in batch.drain(..) {
                        if let Err(response) = self.handle_request(&mut inner, request) {
                            tracing::error!("Failed to handle channel request: {:?}", response);
                        }
                    }
                },
                _ = shutdown.notified() => {
                    tracing::info!("Channel service shutting down due to site shutdown");
                    break;
                },
                else => {
                    tracing::info!("Channel service receiver closed, shutting down");
                    break;
                }
            }
        }
    }
}

/// High-performance single-threaded pubsub implementation.
///
/// Optimizations:
/// 1. Vec-based topic_map: O(n) iteration but cache-friendly, eager cleanup on unsubscribe
/// 2. TopicId interning at boundaries: publish/subscribe intern strings → u32, hotpath uses integer lookups only
/// 3. user_key/channel_key indexes: O(1) bulk unsubscribe instead of O(n) scan
/// 4. Drain-batch processing: amortizes channel overhead (up to 32 requests/batch)
///
/// Data structure design:
/// - subscriptions: main storage, keyed by UUID
/// - topic_map: TopicId → Vec<UUID> for fast publish (no string hashing)
/// - topic_interner: String → TopicId for topic string interning (no Arc<str> overhead)
/// - user_index: user_key → Vec<UUID> for O(1) bulk unsubscribe by user
/// - channel_index: channel_key → Vec<UUID> for O(1) bulk unsubscribe by channel
/// - dead_subscriptions: buffer for cleanup during publish (closed channels)
/// - batch_buffer: reusable buffer for bulk operations (avoids allocations)
struct ChannelInner {
    capacity: usize,
    /// Main subscription storage: UUID → subscription details
    subscriptions: HashMap<uuid::Uuid, LocalSubscription>,
    /// Topic routing: TopicId → list of subscriber UUIDs
    /// Uses Vec instead of HashSet for cache locality (iteration is common)
    topic_map: HashMap<TopicId, Vec<uuid::Uuid>>,
    /// Topic interner: string → TopicId for compact topic representation
    topic_interner: HashMap<String, TopicId>,
    /// User index: user_key → subscriber UUIDs for bulk unsubscribe
    user_index: HashMap<String, Vec<uuid::Uuid>>,
    /// Channel index: channel_key → subscriber UUIDs for bulk unsubscribe
    channel_index: HashMap<String, Vec<uuid::Uuid>>,
    /// Reusable buffer for cleanup during publish (closed subscriber channels)
    dead_subscriptions: Vec<uuid::Uuid>,
    /// Reusable buffer for bulk unsubscribe operations (avoids repeated allocations)
    batch_buffer: Vec<uuid::Uuid>,
    /// Monotonic counter for generating unique TopicIds
    next_topic_id: u32,
}

impl ChannelInner {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            topic_map: HashMap::new(),
            topic_interner: HashMap::new(),
            user_index: HashMap::new(),
            channel_index: HashMap::new(),
            subscriptions: HashMap::new(),
            dead_subscriptions: Vec::with_capacity(16),
            batch_buffer: Vec::with_capacity(32),
            next_topic_id: 0,
        }
    }

    /// Intern a topic string into a TopicId. Idempotent: returns existing ID if present.
    /// This converts string operations into integer operations for the hotpath.
    fn intern_topic(&mut self, topic: &str) -> TopicId {
        if let Some(id) = self.topic_interner.get(topic) {
            return *id;
        }
        // New topic: assign next ID and store mapping
        let id = TopicId(self.next_topic_id);
        self.next_topic_id += 1;
        self.topic_interner.insert(topic.to_string(), id);
        id
    }

    /// Publish an event to a topic. Returns number of subscribers the event was delivered to.
    /// Non-blocking: drops events if subscriber queues are full.
    fn publish(&mut self, topic: String, message: String, user_keys: Option<Vec<String>>) -> usize {
        // Intern the topic string at the boundary
        let topic_id = self.intern_topic(&topic);
        let event = Arc::new(ChannelEvent {
            topic_id,
            topic,
            message,
            user_keys,
        });

        let mut delivered = 0;
        // Lookup by TopicId (u32 hash, not string hash)
        if let Some(sub_ids) = self.topic_map.get(&topic_id) {
            for &sub_id in sub_ids {
                // Skip dead subscriptions (removed but not yet compacted)
                let sub = match self.subscriptions.get(&sub_id) {
                    Some(sub) => {
                        if let Some(ref keys) = event.user_keys {
                            if keys.iter().all(|u| u != &sub.user_key){
                                continue;
                            }
                        }
                        sub
                    }
                    None => continue,
                };
                // Non-blocking send: drops event if queue is full
                if let Err(err) = sub.sender.try_send(event.clone()) {
                    match err {
                        mpsc::error::TrySendError::Full(_) => {
                            tracing::warn!("Subscriber {} channel full, dropping event", sub.id);
                            continue;
                        }
                        mpsc::error::TrySendError::Closed(_) => {
                            // Queue for cleanup: subscriber dropped the receiver
                            self.dead_subscriptions.push(sub.id);
                        }
                    }
                } else {
                    delivered += 1;
                }
            }
        }
        // Eagerly clean up dead subscriptions (prevents memory leaks)
        if !self.dead_subscriptions.is_empty() {
            Self::remove_subscriptions_from(
                &mut self.subscriptions,
                &mut self.topic_map,
                &mut self.user_index,
                &mut self.channel_index,
                &mut self.dead_subscriptions,
            );
        }

        delivered
    }

    /// Subscribe to multiple topics. Returns (subscription_id, event_receiver).
    /// 
    /// Builds forward edges (topic_id → sub_id) and reverse edges (sub → topic_ids)
    /// plus indexes (user_key → sub_id, channel_key → sub_id) for efficient cleanup.
    fn subscribe(
        &mut self,
        user_key: String,
        channel_key: String,
        topics: Vec<String>,
    ) -> (uuid::Uuid, mpsc::Receiver<Arc<ChannelEvent>>) {
        let (tx, rx) = mpsc::channel::<Arc<ChannelEvent>>(self.capacity);
        let sub_id = uuid::Uuid::now_v7();

        // Intern all topics at the boundary
        let topic_ids: Vec<TopicId> = topics.iter().map(|t| self.intern_topic(t)).collect();

        // Store reverse edges: sub → topics (for efficient unsubscribe)
        let subscription = LocalSubscription {
            id: sub_id,
            user_key: user_key.clone(),
            channel_key: channel_key.clone(),
            sender: tx,
            topics: topic_ids.clone(),
        };

        // Build forward edges: topic_id → sub_id (for publish routing)
        for &topic_id in &topic_ids {
            self.topic_map
                .entry(topic_id)
                .or_insert_with(|| Vec::with_capacity(4))
                .push(sub_id);
        }

        // Build user index: user_key → sub_id (for bulk unsubscribe by user)
        self.user_index
            .entry(user_key)
            .or_insert_with(|| Vec::with_capacity(4))
            .push(sub_id);
        // Build channel index: channel_key → sub_id (for bulk unsubscribe by channel)
        self.channel_index
            .entry(channel_key)
            .or_insert_with(|| Vec::with_capacity(4))
            .push(sub_id);

        self.subscriptions.insert(sub_id, subscription);
        (sub_id, rx)
    }

    /// Unsubscribe a single subscription by ID.
    /// Uses reverse edges (sub.topics) to eagerly clean all data structures.
    fn unsubscribe(&mut self, sub_id: uuid::Uuid) -> usize {
        if let Some(sub) = self.subscriptions.remove(&sub_id) {
            // Use reverse edges to clean topic_map
            Self::remove_from_topics(&mut self.topic_map, sub_id, &sub.topics);
            // Clean indexes
            Self::remove_from_index(&mut self.user_index, &sub.user_key, sub_id);
            Self::remove_from_index(&mut self.channel_index, &sub.channel_key, sub_id);
            1
        } else {
            0
        }
    }

    /// Unsubscribe all subscriptions for a given user.
    /// Uses user_index for O(1) lookup instead of O(n) scan.
    fn unsubscribe_by_user(&mut self, user_key: &str) -> usize {
        self.batch_buffer.clear();
        // Take ownership of the ID list from the index (avoids double-lookup)
        if let Some(ids) = self.user_index.remove(user_key) {
            self.batch_buffer.extend(ids);
        }
        Self::remove_subscriptions_from(
            &mut self.subscriptions,
            &mut self.topic_map,
            &mut self.user_index,
            &mut self.channel_index,
            &mut self.batch_buffer,
        )
    }

    /// Unsubscribe all subscriptions for a given channel.
    /// Uses channel_index for O(1) lookup instead of O(n) scan.
    fn unsubscribe_by_channel(&mut self, channel_key: &str) -> usize {
        self.batch_buffer.clear();
        // Take ownership of the ID list from the index (avoids double-lookup)
        if let Some(ids) = self.channel_index.remove(channel_key) {
            self.batch_buffer.extend(ids);
        }
        Self::remove_subscriptions_from(
            &mut self.subscriptions,
            &mut self.topic_map,
            &mut self.user_index,
            &mut self.channel_index,
            &mut self.batch_buffer,
        )
    }

    /// Remove a subscription from all topic vectors using reverse edges.
    /// O(topics × avg_subs_per_topic) with swap_remove for O(1) removal from each Vec.
    fn remove_from_topics(
        topic_map: &mut HashMap<TopicId, Vec<uuid::Uuid>>,
        sub_id: uuid::Uuid,
        topics: &[TopicId],
    ) {
        for &topic_id in topics {
            if let Some(vec) = topic_map.get_mut(&topic_id) {
                // swap_remove is O(1) but doesn't preserve order (okay for our use case)
                if let Some(pos) = vec.iter().position(|&id| id == sub_id) {
                    vec.swap_remove(pos);
                }
                // Clean up empty topic entries to avoid memory leaks
                if vec.is_empty() {
                    topic_map.remove(&topic_id);
                }
            }
        }
    }

    /// Remove a subscription from a user or channel index.
    /// Same strategy as remove_from_topics: swap_remove + cleanup empty entries.
    fn remove_from_index(
        index: &mut HashMap<String, Vec<uuid::Uuid>>,
        key: &str,
        sub_id: uuid::Uuid,
    ) {
        if let Some(vec) = index.get_mut(key) {
            if let Some(pos) = vec.iter().position(|&id| id == sub_id) {
                vec.swap_remove(pos);
            }
            if vec.is_empty() {
                index.remove(key);
            }
        }
    }

    /// Batch remove subscriptions from all data structures.
    /// Used for both bulk unsubscribe and cleaning up closed channels.
    /// Drains the buffer to allow reuse (no allocations on subsequent calls).
    fn remove_subscriptions_from(
        subscriptions: &mut HashMap<uuid::Uuid, LocalSubscription>,
        topic_map: &mut HashMap<TopicId, Vec<uuid::Uuid>>,
        user_index: &mut HashMap<String, Vec<uuid::Uuid>>,
        channel_index: &mut HashMap<String, Vec<uuid::Uuid>>,
        buffer: &mut Vec<uuid::Uuid>,
    ) -> usize {
        let count = buffer.len();
        for sub_id in buffer.drain(..) {
            if let Some(sub) = subscriptions.remove(&sub_id) {
                // Use reverse edges to clean all forward references
                Self::remove_from_topics(topic_map, sub_id, &sub.topics);
                Self::remove_from_index(user_index, &sub.user_key, sub_id);
                Self::remove_from_index(channel_index, &sub.channel_key, sub_id);
            }
        }
        count
    }
}


pub struct BeaconHandler{
    handler: signals::Signaller
}

pub struct BeaconRegistry {
    handlers: Vec<BeaconHandler>,
}

impl BeaconRegistry{

}

pub struct BeaconEngine{
    registry: BeaconRegistry,
    sender: mpsc::Sender<ChannelEvent>,
    service: Arc<LocalChannelService>,
}

impl BeaconEngine {
    // pub fn new(registry: BeaconRegistry) -> Self {
    //     let (sender, receiver) = mpsc::channel(100); // Example buffer size
    //     Self {
    //         registry,
    //         sender,
    //         service: Arc::new(LocalChannelService::new(sender.clone(), "http://localhost:8080".into(), 100)),
    //     }
    // }

    fn handle_signal<P: callables::Payloadable>(&self, item: &P) {
        let data = serde_json::to_string(&item).unwrap_or_default();

        let topic = P::schema_name();

        let event = ChannelEvent {
            topic_id: TopicId(0), // Placeholder, actual TopicId management needed
            topic: topic.to_string(),
            message: data,
            user_keys: None,
        };
        if let Err(err) = self.sender.try_send(event) {
            tracing::error!("Failed to send beacon event: {}", err);
        }
    }

    fn register_signal_handlers(&mut self, signal_registry: &mut signals::SignalRegistry) {
        for handler in self.registry.handlers.drain(..) {
            signal_registry.register(handler.handler);
        }
    }

        /// Axum SSE handler
    // pub async fn subscription_route<I, S>(
    //     self,
    //     user_key: String,
    //     channel_key: String,
    //     topics: Vec<String>,
    // ) -> axum::response::Sse<impl Stream<Item = Result<Event, Infallible>>>
    // {
    //     let (tx, rx) = oneshot::channel();
    //     let receiver = self.service.send(ChannelRequest {
    //         message: ChannelMessage::Subscribe {
    //             user_key,
    //             channel_key,
    //             topics,
    //         },
    //         reply: tx, // Placeholder, actual reply handling needed
    //     }).await; // Handle errors appropriately

    //     let response = rx.await.unwrap(); // Handle errors appropriately

    //     let stream = ReceiverStream::new(receiver).map(|msg| {
    //         Ok(Event::default()
    //             .event(msg.topic.as_ref())
    //             .data(msg.message.as_ref()))
    //     });

    //     Sse::new(stream).keep_alive(KeepAlive::default())
    // }

    pub fn run(self){

    }

}
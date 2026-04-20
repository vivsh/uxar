use std::any::TypeId;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::{collections::HashSet, convert::Infallible};

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use parking_lot::RwLock;
use tokio::signal;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;

use crate::{Site, auth::AuthUser, callables, signals};

#[derive(Clone, Debug)]
pub struct Payload {
    pub topic: Arc<str>,
    pub message: Arc<str>,
}

/// Event target specification
#[derive(Clone, Debug)]
pub enum Target {
    User(Arc<str>),
    RoleMask(u64),
    All,
}

/// Delivery status for internal tracking
#[derive(Clone, Debug)]
enum DeliveryStatus {
    Ok,
    Full,
    Closed,
    Excluded,
}

/// Subscriber representation
#[derive(Debug)]
struct Subscriber {
    id: uuid::Uuid,
    sender: mpsc::Sender<Payload>,
    topics: HashSet<String>,
}

impl Subscriber {
    fn matches(&self, topic: &str) -> bool {
        self.topics.contains(topic)
    }

    fn send(&self, payload: &Payload) -> DeliveryStatus {
        if !self.matches(&payload.topic) {
            return DeliveryStatus::Excluded;
        }

        match self.sender.try_send(payload.clone()) {
            Ok(()) => DeliveryStatus::Ok,
            Err(mpsc::error::TrySendError::Full(_)) => DeliveryStatus::Full,
            Err(mpsc::error::TrySendError::Closed(_)) => DeliveryStatus::Closed,
        }
    }
}

/// Internal state for the beacon
#[derive(Debug)]
struct BeaconInner {
    subscribers: Vec<Subscriber>,
    capacity: usize,
    exclusive: bool,
}

impl BeaconInner {
    fn add_subscriber<S, I>(&mut self, topics: I) -> mpsc::Receiver<Payload>
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        let (sender, receiver) = mpsc::channel(self.capacity);
        let subscriber = Subscriber {
            id: uuid::Uuid::now_v7(),
            sender,
            topics: topics.into_iter().map(|s| s.as_ref().to_string()).collect(),
        };
        self.subscribers.push(subscriber);
        receiver
    }

    fn send_message(&mut self, payload: Payload) {
        self.subscribers
            .retain(|sub| !matches!(sub.send(&payload), DeliveryStatus::Closed));
    }
}

/// Thread-safe event broadcasting system
#[derive(Clone, Debug)]
pub struct Beacon {
    inner: Arc<RwLock<BeaconInner>>,
}

impl Beacon {
    /// Create a new beacon
    pub fn new(capacity: usize, exclusive: bool) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Arc::new(RwLock::new(BeaconInner {
                subscribers: Vec::new(),
                capacity,
                exclusive,
            })),
        }
    }

    /// Subscribe a user and return message receiver
    pub fn subscribe<S, I>(&self, topics: I) -> mpsc::Receiver<Payload>
    where
        S: AsRef<str>,
        I: IntoIterator<Item = S>,
    {
        let mut inner = self.inner.write();
        inner.add_subscriber(topics)
    }

    /// Broadcast message to matching subscribers
    pub fn publish(&self, payload: Payload) {
        let mut inner = self.inner.write();
        inner.send_message(payload);
    }

    /// Get current subscriber count
    pub fn count(&self) -> usize {
        self.inner.read().subscribers.len()
    }

    /// Axum SSE handler
    pub async fn subscription_route<I, S>(
        self,
        topics: I,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let receiver = self.subscribe(topics);
        let stream = ReceiverStream::new(receiver).map(|msg| {
            Ok(Event::default()
                .event(msg.topic.as_ref())
                .data(msg.message.as_ref()))
        });

        Sse::new(stream).keep_alive(KeepAlive::default())
    }
}

pub struct Subscription {
    role_mask: u64,
    signal_type_id: TypeId,
    signaller: signals::Signaller,
}

pub struct SubscriptionManager {
    beacon: Arc<RoleBasedBeacon>,
    subscriptions: Vec<Subscription>,
}

impl SubscriptionManager {
    pub fn new(beacon: Arc<RoleBasedBeacon>) -> Self {
        Self {
            beacon,
            subscriptions: Vec::new(),
        }
    }

    pub fn insert<T: callables::Payloadable + Clone>(&mut self, role_mask: u64) {
        let beacon = self.beacon.clone();
        let signaller = signals::signal::<T, _, _>(
            move |t: callables::Payload<T>| {
                let beacon = beacon.clone();
                async move {
                    let p: &T = &t;
                    beacon.handle_signal(p);
                }
            },
            signals::SignalConf::default(),
        );
        let subscription = Subscription {
            role_mask,
            signal_type_id: TypeId::of::<T>(),
            signaller,
        };
        self.subscriptions.push(subscription);
    }
}

pub struct RoleBasedBeaconUser {
    roles: u64,
    sender: mpsc::Sender<Payload>,
}

pub struct RoleBasedBeacon {
    senders: Arc<RwLock<Vec<RoleBasedBeaconUser>>>,
    subscrptions: Arc<RwLock<HashMap<TypeId, u64>>>,
    capacity: usize,
}

impl RoleBasedBeacon {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            senders: Arc::new(RwLock::new(Vec::new())),
            subscrptions: Arc::new(RwLock::new(HashMap::new())),
            capacity,
        }
    }

    pub fn subscribe(&self, role_set: u64) -> mpsc::Receiver<Payload> {
        let (sender, receiver) = mpsc::channel(self.capacity);
        let mut senders = self.senders.write();
        senders.push(RoleBasedBeaconUser {
            roles: role_set,
            sender,
        });
        receiver
    }

    pub(crate) fn handle_signal<P: callables::Payloadable>(self: Arc<Self>, item: &P) {
        let type_id = TypeId::of::<P>();
        let data = serde_json::to_string(&item).unwrap_or_default();
        let payload = Payload {
            topic: Arc::from(std::any::type_name::<P>()),
            message: Arc::from(data),
        };
        self.publish(type_id, payload);
    }

    fn bind_signals(&self, engine: &mut signals::SignalRegistry, subs: Vec<Subscription>) {
        for sub in subs {
            engine.register(sub.signaller);
        }
    }

    pub fn publish(&self, type_id: TypeId, payload: Payload) {
        let mut senders = self.senders.write();
        if let Some(&role_mask) = self.subscrptions.read().get(&type_id) {
            senders.retain(|user| {
                if (user.roles & role_mask) != 0 {
                    match user.sender.try_send(payload.clone()) {
                        Ok(()) => true,
                        Err(mpsc::error::TrySendError::Full(_)) => true,
                        Err(mpsc::error::TrySendError::Closed(_)) => false,
                    }
                } else {
                    true
                }
            });
        }
    }
}

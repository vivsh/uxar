use std::convert::Infallible;
use std::sync::Arc;

use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::AuthUser;

/// Event target specification
#[derive(Clone, Debug)]
pub enum Target {
    User(u64),
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
    user: AuthUser,
    sender: mpsc::Sender<Arc<str>>,
}

impl Subscriber {
    fn matches(&self, target: &Target) -> bool {
        match target {
            Target::User(uid) => self.user.id == *uid,
            Target::RoleMask(mask) => (self.user.roles & mask) != 0,
            Target::All => true,
        }
    }

    fn send(&self, target: &Target, msg: &Arc<str>) -> DeliveryStatus {
        if !self.matches(target) {
            return DeliveryStatus::Excluded;
        }

        match self.sender.try_send(Arc::clone(msg)) {
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
    fn remove_closed(&mut self) {
        self.subscribers.retain(|sub| !sub.sender.is_closed());
    }

    fn remove_user(&mut self, user_id: u64) {
        self.subscribers.retain(|sub| sub.user.id != user_id);
    }

    fn add_subscriber(&mut self, user: AuthUser) -> mpsc::Receiver<Arc<str>> {
        if self.exclusive {
            self.remove_user(user.id);
        }

        let (sender, receiver) = mpsc::channel(self.capacity);
        self.subscribers.push(Subscriber { user, sender });
        receiver
    }

    fn send_message(&mut self, target: Target, msg: Arc<str>) {
        self.subscribers
            .retain(|sub| !matches!(sub.send(&target, &msg), DeliveryStatus::Closed));
    }

    fn is_user_active(&self, user_id: u64) -> bool {
        self.subscribers
            .iter()
            .any(|sub| sub.user.id == user_id && !sub.sender.is_closed())
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
    pub fn subscribe(&self, user: AuthUser) -> mpsc::Receiver<Arc<str>> {
        let mut inner = self.inner.write();
        inner.remove_closed();
        inner.add_subscriber(user)
    }

    /// Remove a user's subscription
    pub fn unsubscribe(&self, user_id: u64) {
        let mut inner = self.inner.write();
        inner.remove_user(user_id);
    }

    /// Broadcast message to matching subscribers
    pub fn publish(&self, target: Target, msg: Arc<str>) {
        let mut inner = self.inner.write();
        inner.send_message(target, msg);
    }

    /// Get current subscriber count
    pub fn count(&self) -> usize {
        self.inner.read().subscribers.len()
    }

    /// Check if a user is actively subscribed
    pub fn is_active(&self, user_id: u64) -> bool {
        self.inner.read().is_user_active(user_id)
    }

    /// Axum SSE handler
    pub async fn subscription_route(
        user: AuthUser,
        axum::extract::State(bus): axum::extract::State<Beacon>,
    ) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
        let receiver = bus.subscribe(user);
        let stream =
            ReceiverStream::new(receiver).map(|msg| Ok(Event::default().data(msg.as_ref())));

        Sse::new(stream).keep_alive(KeepAlive::default())
    }
}

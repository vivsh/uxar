use parking_lot::RwLock;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

/// Fast concurrent cache for task identities with optional TTL to prevent duplicate processing.
/// Uses HashMap for O(1) lookups and VecDeque for efficient FIFO eviction.
/// Stores each string only once using Arc for shared ownership.
/// Keys with no TTL never expire; keys with TTL expire after the specified duration.
/// Thread-safe with RwLock for efficient concurrent reads.
pub struct IdentityCache {
    inner: Arc<RwLock<CacheInner>>,
}

struct CacheInner {
    keys: HashMap<Arc<str>, Option<Instant>>,
    eviction_queue: VecDeque<(Arc<str>, Option<Instant>)>,
    max_size: usize,
}

impl IdentityCache {
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CacheInner {
                keys: HashMap::with_capacity(max_size),
                eviction_queue: VecDeque::with_capacity(max_size),
                max_size,
            })),
        }
    }

    /// Insert a key with optional TTL. Returns true if key was newly inserted, false if already present and not expired.
    /// Pass `None` for ttl to make the key permanent (never expires).
    pub fn insert(&self, key: &str, ttl: Option<Duration>) -> bool {
        let mut cache = self.inner.write();

        let key: Arc<str> = key.into();
        let now = Instant::now();
        let expiry = ttl.map(|d| now + d);

        // Check if already present and not expired
        if let Some(existing_expiry) = cache.keys.get(&key) {
            match existing_expiry {
                None => return false, // Never expires, still valid
                Some(exp) if now < *exp => return false, // Not yet expired
                Some(_) => {
                    // Expired, remove it
                    cache.keys.remove(&key);
                }
            }
        }

        // Clean expired entries from front of queue
        loop {
            let should_remove = cache.eviction_queue.front().and_then(|(_, expiry)| {
                expiry.map(|exp| now >= exp)
            }).unwrap_or(false);
            
            if should_remove {
                if let Some((front_key, _)) = cache.eviction_queue.pop_front() {
                    cache.keys.remove(&front_key);
                }
            } else {
                break;
            }
        }

        // Evict oldest if at capacity
        if cache.keys.len() >= cache.max_size {
            if let Some((old_key, _)) = cache.eviction_queue.pop_front() {
                cache.keys.remove(&old_key);
            }
        }

        cache.keys.insert(Arc::clone(&key), expiry);
        cache.eviction_queue.push_back((key, expiry));
        true
    }

    /// Check if key exists and has not expired. O(1) lookup with efficient concurrent reads.
    pub fn contains(&self, key: &str) -> bool {
        let cache = self.inner.read();
        cache.keys.get(key).map(|expiry| match expiry {
            None => true, // Never expires
            Some(exp) => Instant::now() < *exp,
        }).unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.inner.read().keys.len()
    }
}

impl Clone for IdentityCache {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for IdentityCache {
    fn default() -> Self {
        Self::new()
    }
}
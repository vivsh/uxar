use std::{borrow::Cow, sync::atomic::AtomicUsize};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum ZoneError {
    #[error("Rate limit exceeded")]
    RateLimited,

    #[error("Maximum concurrency reached")]
    MaxConcurrencyReached,

    #[error("Maximum waiters reached")]
    MaxWaitersReached,
}

/// Rate limiting configuration for a zone
#[derive(Debug, Clone)]
pub struct RateLimit {
    /// Maximum number of requests allowed
    pub max_requests: usize,
    /// Time window for the rate limit
    pub window: Duration,
}

impl RateLimit {
    pub fn new(max_requests: usize, window: Duration) -> Self {
        Self {
            max_requests,
            window,
        }
    }

    pub fn per_second(max_requests: usize) -> Self {
        Self::new(max_requests, Duration::from_secs(1))
    }

    pub fn per_minute(max_requests: usize) -> Self {
        Self::new(max_requests, Duration::from_secs(60))
    }

    pub fn per_hour(max_requests: usize) -> Self {
        Self::new(max_requests, Duration::from_secs(3600))
    }

    fn refill_rate(&self) -> f64 {
        self.max_requests as f64 / self.window.as_secs_f64()
    }
}

/// Configuration for creating a ZonePolicy
#[derive(Debug, Clone, Default)]
pub struct ZoneConf {
    pub rate_limit: Option<RateLimit>,
    pub concurrency: Option<usize>,
    pub waiters: Option<usize>,
}

impl ZoneConf {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_rate_limit(mut self, limit: RateLimit) -> Self {
        self.rate_limit = Some(limit);
        self
    }

    pub fn with_concurrency(mut self, max_concurrent: usize) -> Self {
        self.concurrency = Some(max_concurrent);
        self
    }

    pub fn with_waiters(mut self, max_waiters: usize) -> Self {
        self.waiters = Some(max_waiters);
        self
    }
}

#[derive(Debug)]
struct RateLimitState {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl RateLimitState {
    fn new(limit: &RateLimit) -> Self {
        let capacity = limit.max_requests as f64;
        Self {
            tokens: capacity,
            capacity,
            refill_rate: limit.refill_rate(),
            last_refill: Instant::now(),
        }
    }

    fn can_proceed(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        
        // Refill tokens based on elapsed time
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
        
        // Try to consume one token
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn time_until_token(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        
        // Refill tokens based on elapsed time
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
        
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            // Calculate time needed to accumulate 1.0 token
            let tokens_needed = 1.0 - self.tokens;
            let secs = tokens_needed / self.refill_rate;
            Duration::from_secs_f64(secs)
        }
    }
}

/// Permit that enforces zone policies
pub struct ZonePermit {
    _semaphore_permit: Option<OwnedSemaphorePermit>,
    counter: Arc<AtomicUsize>
}

impl ZonePermit {
    fn new(permit: Option<OwnedSemaphorePermit>, counter: Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            _semaphore_permit: permit,
            counter,
        }
    }
}

impl Drop for ZonePermit {
    fn drop(&mut self) {
        // Notify any waiters that a permit has been released
        self.counter.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}


#[derive(Debug)]
struct ZonePolicyInner {
    name: Cow<'static, str>,
    rate_limit: Option<RateLimit>,
    concurrency: Option<Arc<Semaphore>>,
    rate_state: Option<RwLock<RateLimitState>>,   
    max_waiters: Option<usize>, 
}

/// Zone policy controls rate limiting and concurrency for routes and signal sources
#[derive(Debug, Clone)]
pub struct ZonePolicy {
    inner: Arc<ZonePolicyInner>,
    counter: Arc<AtomicUsize>
}

impl ZonePolicy {
    pub fn new(name: impl Into<Cow<'static, str>>, conf: ZoneConf) -> Self {
        let rate_state = conf.rate_limit.as_ref().map(|limit| {
            RwLock::new(RateLimitState::new(limit))
        });

        let concurrency = conf.concurrency.map(|max| Arc::new(Semaphore::new(max)));

        Self {
            inner: Arc::new(ZonePolicyInner {
                name: name.into(),
                rate_limit: conf.rate_limit,
                concurrency,
                rate_state,
                max_waiters: conf.waiters,
            }),
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn name(&self) -> String {
        self.inner.name.to_string()
    }

    pub fn can_wait(&self) -> bool {
        if let Some(max_waiters) = self.inner.max_waiters {
            let current = self.counter.load(std::sync::atomic::Ordering::Relaxed);
            return current < max_waiters;
        }else{
            return true;
        }
    }

    pub fn rate_limit(&self) -> Option<RateLimit> {
        self.inner.rate_limit.clone()
    }

    pub fn max_concurrency(&self) -> Option<usize> {
        self.inner.concurrency.as_ref().map(|s| s.available_permits())
    }

    /// Validate the zone policy configuration
    pub fn validate(&self) -> Result<(), String> {
        let inner = &self.inner;
        
        if inner.name.is_empty() {
            return Err("Zone name cannot be empty".to_string());
        }

        if let Some(limit) = &inner.rate_limit {
            if limit.max_requests == 0 {
                return Err(format!("Zone '{}': max_requests must be greater than 0", inner.name));
            }
            if limit.window.as_secs() == 0 {
                return Err(format!("Zone '{}': window duration must be greater than 0", inner.name));
            }
        }

        Ok(())
    }

    fn check_rate_limit(&self) -> bool {
        let inner = &self.inner;
        match &inner.rate_state {
            Some(state) => {
                let mut state = state.write();
                state.can_proceed()
            }
            _ => true,
        }
    }

    async fn wait_for_rate_limit(&self, unchecked: bool) -> bool {
        let inner = &self.inner;
        if let Some(state) = &inner.rate_state {
            loop {
                let wait_duration = {
                    let mut state = state.write();
                    let duration = state.time_until_token();
                    // If token available, consume atomically
                    if duration.is_zero() && state.can_proceed() {
                        return true;
                    }
                    duration
                };
                
                if unchecked && !self.can_wait() {
                    return false;
                }
                // Sleep and retry
                tokio::time::sleep(wait_duration).await;


            }
        }
        true
    }

    /// Acquire a zone permit, respecting rate limit and concurrency constraints
    /// Waits (sleeps) until both rate limit and concurrency allow
    pub(crate) async fn acquire_without_waiter_check(&self) -> Result<ZonePermit, ZoneError> {
        // Wait for rate limit
        if !self.wait_for_rate_limit(true).await{
            return Err(ZoneError::MaxWaitersReached);
        }

        // Wait for concurrency semaphore
        let permit = match &self.inner.concurrency {
            Some(sem) => {
                let owned = match sem.clone().acquire_owned().await {
                    Ok(owned) => owned,
                    Err(_) => {
                        return Err(ZoneError::MaxConcurrencyReached);
                    },
                };            
                Some(owned)
            },
            None => None,
        };
        
        Ok(ZonePermit::new(permit, self.counter.clone()))
    }    

    /// Acquire a zone permit, respecting rate limit and concurrency constraints
    /// Waits (sleeps) until both rate limit and concurrency allow
    pub async fn acquire(&self) -> Result<ZonePermit, ZoneError> {
        // Wait for rate limit
        if !self.wait_for_rate_limit(false).await{
            return Err(ZoneError::MaxWaitersReached);
        }

        // Wait for concurrency semaphore
        let permit = match &self.inner.concurrency {
            Some(sem) => {
                let owned = match sem.clone().try_acquire_owned() {
                    Ok(owned) => owned,
                    Err(_) => {
                        if ! self.can_wait() {
                            return Err(ZoneError::MaxWaitersReached);
                        }
                        sem.clone().acquire_owned().await
                            .map_err(|_| ZoneError::MaxConcurrencyReached)?
                    },
                };            
                Some(owned)
            },
            None => None,
        };
        
        Ok(ZonePermit::new(permit, self.counter.clone()))
    }

    /// Try to acquire a zone permit without blocking
    /// Returns None if rate limited or would block on concurrency
    pub fn try_acquire(&self) -> Option<ZonePermit> {
        if !self.check_rate_limit() {
            return None;
        }

        let concurrency = self.inner.concurrency.clone();
        let permit = match concurrency {
            Some(sem) => {
                let owned = sem.try_acquire_owned().ok()?;
                Some(owned)
            }
            None => None,
        };

        Some(ZonePermit::new(permit, self.counter.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_creation() {
        let zone = ZonePolicy::new("test", ZoneConf::new());
        assert_eq!(zone.name(), "test");
        assert!(zone.rate_limit().is_none());
        assert!(zone.max_concurrency().is_none());
    }

    #[test]
    fn zone_with_rate_limit() {
        let conf = ZoneConf::new()
            .with_rate_limit(RateLimit::per_second(10));
        let zone = ZonePolicy::new("limited", conf);
        
        assert!(zone.rate_limit().is_some());
        assert_eq!(zone.rate_limit().unwrap().max_requests, 10);
    }

    #[test]
    fn zone_with_concurrency() {
        let conf = ZoneConf::new()
            .with_concurrency(5);
        let zone = ZonePolicy::new("concurrent", conf);
        
        assert!(zone.max_concurrency().is_some());
    }

    #[test]
    fn rate_limit_enforcement() {
        let conf = ZoneConf::new()
            .with_rate_limit(RateLimit::new(2, Duration::from_secs(1)));
        let zone = ZonePolicy::new("test", conf);
        
        assert!(zone.try_acquire().is_some());
        assert!(zone.try_acquire().is_some());
        assert!(zone.try_acquire().is_none());
    }

    #[tokio::test]
    async fn concurrency_enforcement() {
        let conf = ZoneConf::new()
            .with_concurrency(2);
        let zone = ZonePolicy::new("test", conf);
        
        let _permit1 = zone.acquire().await;
        let _permit2 = zone.acquire().await;
        
        assert!(zone.try_acquire().is_none());
    }
}

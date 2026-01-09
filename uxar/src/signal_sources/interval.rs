use std::time::Duration;

use futures::future::BoxFuture;

use crate::{signals::SignalError, signals::SignalPayload, Site};

/// Signal source that emits signals at regular intervals
/// Emits unit payload `()` every interval
pub struct IntervalSignalSource {
    interval: Duration,
}

impl IntervalSignalSource {
    /// Create a new interval signal source
    ///
    /// # Arguments
    /// * `interval` - Duration between emissions
    pub fn new(interval: Duration) -> Self {
        Self { interval }
    }

    /// Create with interval in seconds
    pub fn every_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    /// Create with interval in minutes
    pub fn every_mins(mins: u64) -> Self {
        Self::new(Duration::from_secs(mins * 60))
    }

    /// Create with interval in hours
    pub fn every_hours(hours: u64) -> Self {
        Self::new(Duration::from_secs(hours * 3600))
    }
}

impl crate::signals::SignalSource for IntervalSignalSource {
    fn poll(&mut self, _site: &Site) -> BoxFuture<'_, Result<Option<SignalPayload>, SignalError>> {
        Box::pin(async move {
            tokio::time::sleep(self.interval).await;
            Ok(Some(SignalPayload::new(())))
        })
    } 
}

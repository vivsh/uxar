use futures::future::BoxFuture;

use crate::{signals::SignalError, signals::SignalPayload, Site};

/// Signal source that emits signals on a cron schedule
/// Emits unit payload `()` at scheduled times
pub struct CronSignalSource {
    schedule: cron::Schedule,
}

impl CronSignalSource {
    /// Create a new cron signal source
    ///
    /// # Arguments
    /// * `schedule` - Cron schedule (e.g., "0 0 * * *" for daily at midnight)
    pub fn new(schedule: cron::Schedule) -> Self {
        Self { schedule }
    }

    /// Parse cron expression and create source
    pub fn from_expr(expr: &str) -> Result<Self, SignalError> {
        let schedule = expr
            .parse::<cron::Schedule>()
            .map_err(|e| SignalError::Other(format!("Invalid cron expression: {}", e).into()))?;
        Ok(Self::new(schedule))
    }
}

impl crate::signals::SignalSource for CronSignalSource {
    fn poll(&mut self, _site: &Site) -> BoxFuture<'_, Result<Option<SignalPayload>, SignalError>> {
        Box::pin(async move {
            let now = chrono::Utc::now();
            let next = self
                .schedule
                .upcoming(chrono::Utc)
                .next()
                .ok_or_else(|| {
                    SignalError::Other("No upcoming scheduled time".to_string().into())
                })?;
            let duration = next.signed_duration_since(now).to_std().map_err(|e| {
                SignalError::Other(
                    format!("Failed to compute duration until next scheduled time: {}", e).into(),
                )
            })?;
            tokio::time::sleep(duration).await;
            Ok(Some(SignalPayload::new(())))
        })
    }
}

use std::{any::Any, borrow::Cow, collections::BTreeMap, fmt, sync::Arc};

use futures::future::BoxFuture;

use crate::{
    Site,
    schemables::{SchemaType, Schemable},
    zones::{ZonePermit, ZonePolicy},
};

#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("JSON parse error: {0}")]
    JsonParse(String),

    #[error("JSON deserialization error for {type_name}: {source}")]
    JsonDeserialize {
        type_name: &'static str,
        source: serde_json::Error,
    },

    #[error("Type mismatch: expected {expected}")]
    TypeMismatch { expected: &'static str },

    #[error("Signal '{signal}' not found")]
    SignalNotFound { signal: String },

    #[error("Signal '{signal}' payload error: {source}")]
    PayloadError { signal: String, source: Box<SignalError> },

    #[error("Serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

enum SignalDataInner {
    Typed(Arc<dyn Any + Send + Sync>),
    Json(serde_json::Value),
}

/// Type-safe signal payload with JSON interop and efficient sharing
/// Supports typed Arc payloads or raw JSON for sources without type info
#[derive(Clone)]
pub struct SignalPayload {
    inner: Arc<SignalDataInner>,
}

impl SignalPayload {
    /// Create from typed value
    pub fn new<T: Send + Sync + 'static>(value: T) -> Self {
        Self {
            inner: Arc::new(SignalDataInner::Typed(Arc::new(value))),
        }
    }

    /// Create from JSON value (stores raw, deserializes when dispatched)
    pub fn from_json(value: serde_json::Value) -> Self {
        Self {
            inner: Arc::new(SignalDataInner::Json(value)),
        }
    }

    /// Create from JSON string (parses and stores raw, deserializes when dispatched)
    pub fn from_json_str(json: &str) -> Result<Self, SignalError> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| SignalError::JsonParse(e.to_string()))?;
        Ok(Self::from_json(value))
    }

    /// Get typed reference (only works for the Typed variant)
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        match &*self.inner {
            SignalDataInner::Typed(arc) => (&**arc as &(dyn Any + 'static)).downcast_ref::<T>(),
            SignalDataInner::Json(_) => None,
        }
    }

    pub(crate) fn to_typed<T>(&self) -> Result<T, SignalError>
    where
        T: Clone + serde::de::DeserializeOwned + 'static,
    {
        match &*self.inner {
            SignalDataInner::Typed(arc) => (&**arc as &(dyn Any + 'static))
                .downcast_ref::<T>()
                .cloned()
                .ok_or_else(|| SignalError::TypeMismatch {
                    expected: std::any::type_name::<T>(),
                }),
            SignalDataInner::Json(val) => serde_json::from_value(val.clone()).map_err(|e| {
                SignalError::JsonDeserialize {
                    type_name: std::any::type_name::<T>(),
                    source: e,
                }
            }),
        }
    }

    /// Convert to JSON if type supports serialization
    pub fn to_json<T>(&self) -> Result<serde_json::Value, SignalError>
    where
        T: serde::Serialize + 'static,
    {
        match &*self.inner {
            SignalDataInner::Typed(arc) => (&**arc as &(dyn Any + 'static))
                .downcast_ref::<T>()
                .ok_or_else(|| SignalError::TypeMismatch {
                    expected: std::any::type_name::<T>(),
                })
                .and_then(|v| serde_json::to_value(v).map_err(SignalError::Serialize)),
            SignalDataInner::Json(val) => Ok(val.clone()),
        }
    }

    /// Access raw Arc for internal use (only for Typed variant)
    pub(crate) fn as_arc(&self) -> Option<&Arc<dyn Any + Send + Sync>> {
        match &*self.inner {
            SignalDataInner::Typed(arc) => Some(arc),
            SignalDataInner::Json(_) => None,
        }
    }
}

impl fmt::Debug for SignalPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalData").finish_non_exhaustive()
    }
}

pub struct SignalReceiver {
    pub name: Cow<'static, str>,
    pub schema: SchemaType,
    pub receiver: Arc<
        dyn Fn(Site, SignalPayload) -> BoxFuture<'static, Result<(), SignalError>> + Send + Sync,
    >,
}

impl fmt::Debug for SignalReceiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalReceiver")
            .field("name", &self.name)
            .field("schema", &self.schema)
            .field("receiver", &"<function>")
            .finish()
    }
}

impl Clone for SignalReceiver {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            schema: self.schema.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

/// Container for signal handlers and scheduling
#[derive(Debug, Clone)]
pub struct SignalEngine {
    signals: BTreeMap<String, Vec<SignalReceiver>>,
}

impl SignalEngine {
    pub fn new() -> Self {
        Self {
            signals: BTreeMap::new(),
        }
    }

    /// Register a signal handler
    /// Handler receives typed payload directly - wrapper does Box::pin
    /// Supports both Typed and JSON variants (JSON is deserialized on dispatch)
    pub fn register<F, Fut, T>(&mut self, name: &'static str, receiver: F)
    where
        F: Fn(Site, T) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), SignalError>> + Send + 'static,
        T: Any + Schemable + Clone + serde::de::DeserializeOwned + Send + Sync + 'static,
    {
        let schema = T::schema_type();
        let signal_name = name;

        let erased: Arc<
            dyn Fn(Site, SignalPayload) -> BoxFuture<'static, Result<(), SignalError>>
                + Send
                + Sync,
        > = Arc::new(move |site, data| match data.to_typed::<T>() {
            Ok(payload) => Box::pin(receiver(site, payload)),
            Err(e) => Box::pin(async move {
                Err(SignalError::PayloadError {
                    signal: signal_name.to_string(),
                    source: Box::new(e),
                })
            }),
        });

        self.signals
            .entry(name.to_string())
            .or_default()
            .push(SignalReceiver {
                name: Cow::Borrowed(name),
                schema,
                receiver: erased,
            });
    }

    /// Merge another signal engine into this one
    pub fn merge(&mut self, other: SignalEngine) {
        for (signal_name, receivers) in other.signals {
            self.signals
                .entry(signal_name)
                .or_default()
                .extend(receivers);
        }
    }

    /// Dispatch typed value as signal
    pub async fn dispatch<T>(&self, site: Site, name: &str, payload: T) -> Result<(), SignalError>
    where
        T: Send + Sync + 'static,
    {
        let data = SignalPayload::new(payload);
        self.dispatch_data(site, name, data).await
    }

    /// Dispatch SignalData directly
    pub(crate) async fn dispatch_data(
        &self,
        site: Site,
        name: &str,
        data: SignalPayload,
    ) -> Result<(), SignalError> {
        if let Some(receivers) = self.signals.get(name) {
            for receiver in receivers {
                (receiver.receiver)(site.clone(), data.clone()).await?;
            }
        }
        Ok(())
    }

    /// Dispatch from JSON value
    pub async fn dispatch_json(
        &self,
        site: Site,
        name: &str,
        json: serde_json::Value,
    ) -> Result<(), SignalError> {
        let data = SignalPayload::from_json(json);
        self.dispatch_data(site, name, data).await
    }

    /// Dispatch from JSON string
    pub async fn dispatch_json_str(
        &self,
        site: Site,
        name: &str,
        json: &str,
    ) -> Result<(), SignalError> {
        let data = SignalPayload::from_json_str(json)?;
        self.dispatch_data(site, name, data).await
    }

    /// Dispatch spawning each handler in separate tokio task
    pub fn dispatch_spawn(
        &self,
        site: Site,
        name: &str,
        data: SignalPayload,
    ) -> Result<(), SignalError> {
        let receivers = self.signals.get(name).ok_or_else(|| {
            SignalError::SignalNotFound {
                signal: name.to_string(),
            }
        })?;

        for receiver in receivers {
            let site = site.clone();
            let data = data.clone();
            let receiver = receiver.receiver.clone();

            tokio::spawn(async move {
                if let Err(e) = receiver(site, data).await {
                    tracing::error!("Signal handler error: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Iterate over all registered signal names and their handlers
    pub fn iter_signals(&self) -> impl Iterator<Item = (&str, &[SignalReceiver])> {
        self.signals
            .iter()
            .map(|(name, receivers)| (name.as_str(), receivers.as_slice()))
    }

    /// Get signal receivers for a specific signal name
    pub fn get_signal(&self, name: &str) -> Option<&[SignalReceiver]> {
        self.signals.get(name).map(|v| v.as_slice())
    }
}

impl Default for SignalEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for signal sources that fetch payloads and emit signals
/// Object-safe for storage in Vec<Box<dyn SignalSource>>
/// Supports DB polling, cron/periodic triggers, and postgres NOTIFY relay
pub trait SignalSource: Send + Sync + 'static {
    /// Fetch payloads as SignalPayload (already type-erased and Arc-wrapped)
    ///
    fn poll(&mut self, site: &Site) -> BoxFuture<'_, Result<Option<SignalPayload>, SignalError>>;

    /// Run the signal source continuously until shutdown signal
    /// Fetches payloads in bulk, then emits one-by-one respecting zone policy
    fn run(
        mut self: Box<Self>,
        site: Site,
        signal_name: &'static str,
        engine: Arc<SignalEngine>,
        shutdown: Arc<tokio::sync::Notify>,
        interval: Option<std::time::Duration>,
        zone_policy: ZonePolicy,
    ) -> BoxFuture<'static, ()> {
        Box::pin(async move {
            let mut permit = Option::<ZonePermit>::None;
            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        tracing::info!("Signal source shutting down");
                        break;
                    },
                    _ = zone_policy.acquire()=>{
                        permit = Some(zone_policy.acquire().await);
                    },
                    result = self.poll(&site), if permit.is_some() => {
                        match result {
                            Ok(Some(payload)) => {
                                if let Err(e) = engine.dispatch_spawn(site.clone(), signal_name, payload) {
                                    tracing::error!("Signal dispatch error for '{}': {}", signal_name, e);
                                }
                            },
                            Err(e) => {
                                tracing::error!("Signal source poll error for '{}': {}. Exiting the loop", signal_name, e);
                                break;
                            },
                            _=>{
                                // lose the permit if no payload
                                // so that other zone tasks can run
                                // This releases the concurrency slot but affects the rate limit
                            }
                        }
                        permit = None;
                        if let Some(interval) = interval {
                            tokio::time::sleep(interval).await;
                        }
                    }
                }
            }
        })
    }
}

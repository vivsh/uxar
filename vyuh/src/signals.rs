use crate::{
    Error, Site,
    callables::{self, CallError, Callable, DataBox, HasSite, IntoDataBox},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::{any::TypeId, collections::HashMap, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct SignalHandler {
    type_id: TypeId,
    func: Callable<SignalContext, Error>,
}

impl SignalHandler {
    async fn call(&self, ctx: SignalContext) -> Result<(), Error> {
        self.func.call(ctx).await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct SignalContext {
    site: Site,
    payload: DataBox,
}

impl HasSite for SignalContext {
    fn site(&self) -> &Site {
        &self.site
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SignalConf {}

pub(crate) struct Signaller {
    pub(crate) handler: SignalHandler,
    pub(crate) options: SignalConf,
}

impl Signaller {
    pub(crate) fn operation(&self) -> crate::callables::Operation {
        let spec = self.handler.func.inspect();
        crate::callables::Operation::from_specs(crate::callables::OperationKind::Signal, spec)
            .with_conf(&self.options)
    }
}

pub(crate) fn signal<T, H, Args>(handler: H, options: SignalConf) -> Signaller
where
    T: callables::DataValue,
    H: callables::Specable<Args> + Send + Sync + 'static,
    H::Output: callables::IntoOutput<Error> + callables::IntoReturnPart + Send + 'static,
    Args: callables::FromContext<SignalContext>
        + callables::IntoArgSpecs
        + callables::HasData<T>
        + Send
        + 'static,
{
    let callable = Callable::new(handler);

    Signaller {
        handler: SignalHandler {
            func: callable,
            type_id: TypeId::of::<T>(),
        },
        options,
    }
}

impl IntoDataBox for SignalContext {
    fn into_data_box(self) -> DataBox {
        self.payload
    }
}

/// Errors that can occur during signal registration and dispatch.
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("Signal data type mismatch")]
    DataTypeMismatch,

    #[error("Signal not found")]
    NotFound,

    #[error(transparent)]
    CallError(#[from] CallError),
}

/// Central registry for signal handlers that dispatches events asynchronously.
pub struct SignalRegistry {
    handlers: HashMap<TypeId, Vec<SignalHandler>>,
}

impl fmt::Debug for SignalRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignalRegistry")
            .field("handlers", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl SignalRegistry {
    /// Creates a new, empty signal engine.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Registers a handler for the named signal; all handlers for a signal must use the same data type.
    pub(crate) fn register(&mut self, signaller: Signaller) {
        let type_id = signaller.handler.type_id;
        let entry = self.handlers.entry(type_id).or_default();
        entry.push(signaller.handler);
    }

    pub(crate) fn merge(&mut self, other: SignalRegistry) {
        for (name, other_container) in other.handlers.into_iter() {
            let entry = self.handlers.entry(name).or_default();
            entry.extend(other_container);
        }
    }

    /// Creates a signal engine from this registry.
    /// Any changes to the registry after this call will not affect the engine.
    pub fn engine(&self) -> SignalEngine {
        let registry = Self {
            handlers: self.handlers.clone(),
        };
        SignalEngine::new(registry)
    }
}

/// Site-scoped signal client.
///
/// Signals are fire-and-forget in-process notifications. Submitting a signal
/// validates that a handler exists, then queues dispatch on the site's runtime.
/// Vyuh does not guarantee delivery, ordering, retries, durability, or handler
/// completion for signals.
#[derive(Clone)]
pub struct SignalClient {
    site: Site,
    engine: SignalEngine,
}

impl SignalClient {
    pub(crate) fn new(site: Site, engine: SignalEngine) -> Self {
        Self { site, engine }
    }

    /// Queues a signal for immediate in-process dispatch.
    pub fn submit<T>(&self, item: T) -> Result<(), SignalError>
    where
        T: callables::DataValue,
    {
        let payload = DataBox::new(item);
        self.submit_data(payload)
    }

    /// Queues a signal for delayed in-process dispatch.
    ///
    /// Scheduled signals are cancelled when the site shuts down. They are not
    /// durable and are not retried.
    pub fn schedule<T>(&self, item: T, delay: Duration) -> Result<(), SignalError>
    where
        T: callables::DataValue,
    {
        let payload = DataBox::new(item);
        self.ensure_data_handler(&payload)?;

        let site = self.site.clone();
        let engine = self.engine.clone();
        let shutdown = self.site.shutdown_notifier();
        self.site.spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(delay) => {
                    engine.dispatch_data_fire_and_forget(site, payload).await;
                }
                _ = shutdown.notified() => {}
            }
        });
        Ok(())
    }

    fn submit_data(&self, payload: DataBox) -> Result<(), SignalError> {
        self.ensure_data_handler(&payload)?;

        let site = self.site.clone();
        let engine = self.engine.clone();
        self.site.spawn(async move {
            engine.dispatch_data_fire_and_forget(site, payload).await;
        });
        Ok(())
    }

    fn ensure_data_handler(&self, payload: &DataBox) -> Result<(), SignalError> {
        if self.engine.has_handler(payload.payload_type_id()) {
            Ok(())
        } else {
            Err(SignalError::NotFound)
        }
    }
}

#[derive(Clone)]
pub struct SignalEngine {
    registry: Arc<SignalRegistry>,
}

impl SignalEngine {
    pub fn new(registry: SignalRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    pub(crate) fn has_handler(&self, type_id: TypeId) -> bool {
        self.registry.handlers.contains_key(&type_id)
    }

    pub(crate) async fn dispatch_data_fire_and_forget(&self, site: Site, payload: DataBox) {
        if let Err(err) = self.dispatch_payload(site, payload).await {
            tracing::error!("Error dispatching signal: {}", err);
        }
    }

    /// Dispatches a signal with the given data to all registered handlers asynchronously.
    pub(crate) async fn dispatch_payload(
        &self,
        site: Site,
        payload: DataBox,
    ) -> Result<(), SignalError> {
        let type_id = payload.payload_type_id();
        let handlers = match self.registry.handlers.get(&type_id) {
            Some(handlers) => handlers,
            None => return Err(SignalError::NotFound),
        };

        for handler in handlers.iter() {
            let ctx = SignalContext {
                site: site.clone(),
                payload: payload.clone(),
            };
            if let Err(err) = handler.call(ctx).await {
                tracing::error!("Error executing signal handler: {}", err);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::callables::Data;
    use schemars::JsonSchema;

    #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
    struct TestSignal {
        value: usize,
    }

    async fn record_signal(_payload: Data<TestSignal>) {}

    #[test]
    fn direct_registration_records_signal_operation() {
        let signaller = signal::<TestSignal, _, _>(record_signal, SignalConf::default());
        let op = signaller.operation();

        assert_eq!(op.kind, crate::callables::OperationKind::Signal);
        assert!(!op.hidden);
        assert_eq!(op.path, "");
    }

    #[test]
    fn registry_engine_detects_registered_data_type() {
        let signaller = signal::<TestSignal, _, _>(record_signal, SignalConf::default());
        let mut registry = SignalRegistry::new();
        registry.register(signaller);

        let engine = registry.engine();
        assert!(engine.has_handler(TypeId::of::<TestSignal>()));
        assert!(!engine.has_handler(TypeId::of::<String>()));
    }

    #[test]
    fn merge_appends_handlers_for_same_data_type() {
        async fn first(_payload: Data<TestSignal>) {}
        async fn second(_payload: Data<TestSignal>) {}

        let mut left = SignalRegistry::new();
        left.register(signal::<TestSignal, _, _>(first, SignalConf::default()));

        let mut right = SignalRegistry::new();
        right.register(signal::<TestSignal, _, _>(second, SignalConf::default()));

        left.merge(right);

        assert_eq!(
            left.handlers.get(&TypeId::of::<TestSignal>()).map(Vec::len),
            Some(2)
        );
    }
}

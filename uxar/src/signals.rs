

use std::{fmt, str, sync::atomic::AtomicBool};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    any::{Any, TypeId},
    collections::{HashMap},
    sync::Arc,
};
use crate::{
    Site, callables::{self, CallError, Callable, IntoPayloadData, PayloadData},
    debounce::{DebounceCall, DebounceConf},
};



#[derive(Clone)]
pub struct SignalHandler{
    type_id: TypeId,
    func: Callable<SignalContext, SignalError>,
    debouncer: Option<DebounceCall<SignalContext, SignalError>>,
}

impl SignalHandler {
    async fn call(&self, ctx: SignalContext) -> Result<(), SignalError> {
        if let Some(d) = &self.debouncer {
            d.trigger(ctx);
        } else {
            self.func.call(ctx).await?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct SignalContext {
    site: Site,
    payload: PayloadData,
}


#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SignalConf {
    debounce: Option<DebounceConf>
}

pub(crate) struct Signaller{
    pub(crate) handler: SignalHandler,
    pub(crate) options: SignalConf,
}


pub(crate) fn signal<T, H, Args>(handler: H, options: SignalConf) -> Signaller
where
    T: callables::Payloadable,
    H: callables::Specable<Args, Output = ()> + Send + Sync + 'static,
    Args: callables::FromContext<SignalContext> + callables::IntoArgSpecs + callables::HasPayload<T> + Send + 'static,
{
    let callable = Callable::new(handler);

    let debouncer = options.debounce.as_ref().map(|conf| {
        DebounceCall::new(conf.clone(), callable.clone())
    });

    Signaller {
        handler: SignalHandler { func: callable, type_id: TypeId::of::<T>(), debouncer },
        options,
    }
}

impl IntoPayloadData for SignalContext {
    fn into_payload_data(self) -> PayloadData {
        self.payload
    }
}

/// Errors that can occur during signal registration and dispatch.
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("Signal payload type mismatch")]
    PayloadTypeMismatch,

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

    /// Registers a handler for the named signal; all handlers for a signal must use the same payload type.
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

/// Scoped helper for dispatching signals with a bound site and manager reference.
pub struct SignalDispatcher<'a> {
    site: &'a Site,
    engine: &'a SignalEngine,
}

impl<'a> SignalDispatcher<'a> {
    /// Dispatches a signal with the given payload using the bound site context.
    pub async fn dispatch<T>(&self, item: T) -> Result<(), SignalError>
    where
        T: Any + Send + Sync + Serialize + JsonSchema + 'static,
    {
        let site = self.site.clone();
        self.engine.dispatch(site, item).await
    }

    pub async fn dispatch_payload(
        &self,
        payload: PayloadData,
    ) -> Result<(), SignalError> {
        let site = self.site.clone();
        self.engine.dispatch_payload(site, payload).await
    }

    // pub async fn dispatch_payload_by_spawn(
    //     &self,
    //     payload: PayloadData,
    // ) -> Result<(), SignalError> {
    //     let site = self.site.clone();
    //     self.engine.clone()
    //         .dispatch_payload_by_spawn(site, payload)
    // }
}

// #[derive(Clone)]
pub struct SignalEngine {
    registry: Arc<SignalRegistry>,
}

impl SignalEngine {
    pub fn new(registry: SignalRegistry) -> Self {
        Self {
            registry: Arc::new(registry),
        }
    }

    /// Dispatches a signal with the given payload to all registered handlers asynchronously.
    async fn dispatch<T: 'static>(&self, site: Site, item: T) -> Result<(), SignalError>
    where
        T: Any + Send + Sync + Serialize + JsonSchema + 'static,
    {
        let payload = PayloadData::new(item);
        self.dispatch_payload(site, payload)
            .await
    }

    /// Dispatches a signal with the given payload to all registered handlers asynchronously.
    fn dispatch_payload_by_spawn(
        &self,
        site: Site,
        payload: PayloadData,
    ) -> Result<(), SignalError> {
        if !self
            .registry
            .handlers
            .contains_key(&payload.payload_type_id())
        {
            return Err(SignalError::NotFound);
        }
        let engine = Self{
            registry: self.registry.clone(),
        };
        tokio::spawn(async move {
            if let Err(err) = engine.dispatch_payload(site, payload).await{
                tracing::error!("Error dispatching signal: {}", err);
            }
        });
        Ok(())
    }

    /// Dispatches a signal with the given payload to all registered handlers asynchronously.
    pub(crate) async fn dispatch_payload(
        &self,
        site: Site,
        payload: PayloadData,
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
            if let Err(err) = handler.call(ctx).await{
                tracing::error!("Error executing signal handler: {}", err);
            }
        }

        Ok(())
    }
}


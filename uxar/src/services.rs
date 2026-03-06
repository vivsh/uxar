use std::{any::TypeId, pin::Pin, sync::Arc};

use indexmap::IndexMap;

use crate::{Site, callables, site::PartialSite};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service already registered")]
    AlreadyRegistered,

    #[error("Handler returned an unexpected output type")]
    UnexpectedOutput,

    #[error("Service Arc was shared when exclusive access was required during build")]
    ArcShared,

    #[error("Facade downcast failed: stored type did not match expected Arc<T>")]
    FacadeDowncast,

    #[error(transparent)]
    CallError(#[from] callables::CallError),
}

pub struct ServiceExposer<T: Send + Sync + 'static> {
    _marker: std::marker::PhantomData<T>,
    facades: Vec<ServiceFacade>,
}

impl<T: Send + Sync + 'static> ServiceExposer<T> {
    fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            facades: Vec::new(),
        }
    }

    pub fn expose<U: ?Sized + Send + Sync + 'static>(
        &mut self,
        coercer: impl Fn(Arc<T>) -> Arc<U> + Send + Sync + 'static,
    ) -> Result<(), ServiceError> {
        let facade = ServiceFacade {
            type_id: TypeId::of::<U>(),
            // `obj` is an Arc<dyn Any> whose concrete type is Arc<T>.
            // We downcast it to Arc<T>, then apply the user-supplied coercer.
            coerce_fn: Box::new(move |obj: AnyArc| {
                let t: Arc<T> = obj
                    .downcast::<T>()
                    .map_err(|_| ServiceError::FacadeDowncast)?;
                let u: Arc<U> = coercer(t);
                Ok(Box::new(u) as AnyBox)
            }),
        };
        self.facades.push(facade);
        Ok(())
    }
}

pub struct ServiceRunner {
    workers: Vec<ServiceWorker>,
}

impl ServiceRunner {

    fn new() -> Self {
        Self { workers: Vec::new() }
    }

    pub fn run<H, Args>(&mut self, name: &str, handler: H) -> Result<(), ServiceError> 
    where 
        H: callables::Specable<Args, Output = Result<(), ServiceError>> + Send + Sync + 'static,
        Args: callables::FromContext<ServiceWorkContext> + callables::IntoArgSpecs + Send + 'static,
    {
        let worker = ServiceWorker::from_callable(name.to_string(), handler);
        self.workers.push(worker);
        Ok(())
    }
}

#[derive(Clone)]
pub struct ServiceBuildContext {
    site: PartialSite,
}

#[derive(Clone)]
pub struct ServiceWorkContext {
    site: Site,
}

#[derive(Clone)]
struct ServiceWorker {
    name: String,
    func: callables::Callable<ServiceWorkContext, ServiceError>,
}

impl ServiceWorker {
    fn from_callable<H, Args>(name: String, handler: H) -> Self
    where
        H: callables::Specable<Args, Output = Result<(), ServiceError>> + Send + Sync + 'static,
        Args: callables::FromContext<ServiceWorkContext> + callables::IntoArgSpecs + Send + 'static,
    {
        let callable = callables::Callable::new(handler);

        ServiceWorker { name, func: callable }
    }

    async fn call(&self, ctx: ServiceWorkContext) -> Result<(), ServiceError> {
        self.func.call(ctx).await?;
        Ok(())
    }
}

type AnyBox = Box<dyn std::any::Any + Send + Sync>;
type AnyArc = Arc<dyn std::any::Any + Send + Sync>;

struct ServiceFacade {
    type_id: TypeId,
    // Receives a clone of the concrete Arc<T> (type-erased) and produces the
    // coerced Arc<U> for the requested interface, also type-erased.
    coerce_fn: Box<dyn Fn(AnyArc) -> Result<AnyBox, ServiceError> + Send + Sync>,
}

// Stored inside the serviceengine
struct ServiceEntry {
    inner: AnyArc,
    workers: Vec<ServiceWorker>,
    facades: Vec<ServiceFacade>,
}

#[derive(Clone)]
pub struct ServiceHandler {
    type_id: TypeId,
    build_fn: Arc<
        dyn Fn(
                ServiceBuildContext,
            ) -> Pin<
                Box<dyn std::future::Future<Output = Result<ServiceEntry, ServiceError>> + Send>,
            > + Send + Sync,
    >,
}

impl ServiceHandler {
    pub fn new<T, H, Args>(handler: H) -> Self
    where
        T: Service,
        H: callables::Specable<Args, Output = Agent<T>> + Send + Sync + 'static,
        Args:
            callables::FromContext<ServiceBuildContext> + callables::IntoArgSpecs + Send + 'static,
    {
        let callable: callables::Callable<ServiceBuildContext, ServiceError> =
            callables::Callable::new(handler);

        let build_fn = move |ctx: ServiceBuildContext| {
            let callable = callable.clone();
            Box::pin(async move {
                let output = callable.call(ctx).await?;

                let inner = output.into_any_arc();

                let mut inner_svc: Arc<T> = inner
                    .downcast::<T>()
                    .map_err(|_| ServiceError::UnexpectedOutput)?;

                let agent = Arc::get_mut(&mut inner_svc).ok_or(ServiceError::ArcShared)?;
                let mut exposer = ServiceExposer::<T>::new();
                // Expose the concrete type itself for internal use. This allows services to depend
                // on each other using their concrete types, while still exposing only the intended 
                // interfaces to the service engine.
                exposer.expose(std::convert::identity)?;
                T::expose(&mut exposer)?;
                let mut sr = ServiceRunner::new();
                agent.run(&mut sr)?;

                let inner_t: Arc<T> = inner_svc;

                Ok(ServiceEntry {
                    inner: inner_t as AnyArc,
                    workers: sr.workers,
                    facades: exposer.facades,
                })
            })
                as Pin<
                    Box<
                        dyn std::future::Future<Output = Result<ServiceEntry, ServiceError>> + Send,
                    >,
                >
        };

        ServiceHandler {
            type_id: TypeId::of::<T>(),
            build_fn: Arc::new(build_fn),
        }
    }
}

pub trait Service: Sized + Send + Sync + 'static {
    fn expose(exposer: &mut ServiceExposer<Self>) -> Result<(), ServiceError>{
        Ok(())
    }

    fn run(&mut self, runner: &mut ServiceRunner) -> Result<(), ServiceError>{
        Ok(())
    }
}

pub struct Agent<T: Service>(pub T);

impl<E, T: Service> callables::IntoOutput<E> for Agent<T> {
    fn into_output(self) -> Result<callables::PayloadData, E> {
        Ok(callables::PayloadData::new(self.0))
    }
}

impl<T: Service> callables::IntoReturnPart for Agent<T> {
    fn into_return_part() -> callables::ReturnPart {
        callables::ReturnPart::Empty
    }
}

impl<T: Service> From<T> for Agent<T> {
    fn from(t: T) -> Self {
        Agent(t)
    }
}


#[derive(Clone)]
pub struct ServiceRegistry {
    services: IndexMap<TypeId, ServiceHandler>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: IndexMap::new(),
        }
    }

    pub(crate) fn register(&mut self, entry: ServiceHandler) -> Result<(), ServiceError> {
        let type_id = entry.type_id;
        if self.services.contains_key(&type_id) {
            return Err(ServiceError::AlreadyRegistered);
        }
        self.services.insert(type_id, entry);
        Ok(())
    }

    pub(crate)  fn merge(&mut self, other: ServiceRegistry) -> Result<(), ServiceError> {
        for entry in other.services.into_values() {
            self.register(entry)?;
        }
        Ok(())
    }
}

pub struct ServiceEngine {
    services: IndexMap<TypeId, AnyBox>,
    workers: Vec<ServiceWorker>,
}

impl ServiceEngine {
    pub fn new() -> Self {
        Self {
            services: IndexMap::new(),
            workers: vec![],
        }
    }

    pub(crate) async fn load(&mut self, registry: ServiceRegistry, partial_site: PartialSite) -> Result<(), ServiceError> {
        for (_, handler) in registry.services {
            let entry_future = (handler.build_fn)(ServiceBuildContext { site: partial_site.clone() });
            let entry = entry_future.await?;
            for facade in entry.facades {
                let iface = (facade.coerce_fn)(entry.inner.clone())?;
                if self.services.contains_key(&facade.type_id) {
                    return Err(ServiceError::AlreadyRegistered);
                }
                self.services.insert(facade.type_id, iface);
            }
            self.workers.extend(entry.workers);
        }
        Ok(())
    }

    pub(crate) async fn start_workers(&self, site: Site, joinset: &mut tokio::task::JoinSet<()>) -> Result<(), ServiceError> {
        for worker in &self.workers {
            let ctx = ServiceWorkContext { site: site.clone() };
            let worker = worker.clone();
            joinset.spawn(async move { 
                if let Err(e) =  worker.call(ctx).await{
                    tracing::error!("Service worker {} failed with error: {:?}", worker.name, e);
                }
            });
        }
        Ok(())
    }

    pub fn get<T: ?Sized + 'static>(&self) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        self.services
            .get(&type_id)
            .and_then(|boxed_iface| boxed_iface.downcast_ref::<Arc<T>>().cloned())
    }
}

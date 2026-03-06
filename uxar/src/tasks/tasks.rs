use std::{
    any::TypeId,
    borrow::Cow,
    collections::{HashMap, VecDeque},
    future::Future,
    sync::Arc,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{OwnedSemaphorePermit, mpsc};

use crate::{
    Site,
    callables::{self, Callable},
    db::Pool, tasks::{indentity::IdentityCache, store::AbstractTaskStore},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConf {
    pub poll_interval_ms: u32,
    pub capacity: usize,
    pub concurrency: usize,
    pub batch_size: usize,
}

impl Default for TaskConf {
    fn default() -> Self {
        Self {
            poll_interval_ms: 15000,
            capacity: 1000,
            concurrency: 10,
            batch_size: 250,
        }
    }
}

pub struct TaskContext {
    site: Site,
    payload: callables::PayloadData,
}

impl callables::IntoPayloadData for TaskContext {
    fn into_payload_data(self) -> callables::PayloadData {
        self.payload
    }
}

type TaskHandler = Callable<TaskContext, TaskError>;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Type mismatch: expected {0}, got {1}")]
    TypeMismatch(String, String),

    #[error("Illegal task kind for flow execution")]
    UnexpectedFlowKind,

    #[error("Task '{0}' not found")]
    TaskNotFound(String),

    #[error("Task JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Task execution error: {0}")]
    TaskExecutionError(String),

    #[error("Task already exists: {0}")]
    AlreadyExists(String),

    #[error("Identity already exists")]
    IdentityError,

    #[error(transparent)]
    CallError(#[from] crate::callables::CallError),

    #[error("Unknown task error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type)]
#[repr(i16)]
pub enum TaskKind {
    Flow,
    Unit,
}


#[derive(Debug, Default)]
pub struct TaskInputConf{
    pub delay_ms: Option<u32>,
    pub identity: Option<String>,
    pub transient: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::Type)]
#[repr(i16)]
pub enum TaskStatus {
    Ready,
    Enqueued,
    Suspend,
    Success,
    Failure,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskInput {
    pub id: uuid::Uuid,
    pub root_id: uuid::Uuid,
    pub name: String,
    pub child_id: Option<uuid::Uuid>,
    pub data: String,
    pub state: Option<String>,
    pub ready_time: chrono::DateTime<chrono::Utc>,
    pub status: TaskStatus,
    pub identity: Option<String>,
}

impl TaskInput {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn to<T>(&self) -> Result<T, TaskError>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_str(&self.data).map_err(TaskError::from)
    }
}

#[derive(Debug, Clone)]
pub enum TaskFlowOutput {
    Suspend(uuid::Uuid),
    Success(String),
}

impl callables::IntoOutput<TaskError> for TaskFlowOutput {
    fn into_output(self) -> Result<callables::PayloadData, TaskError> {
        Ok(callables::PayloadData::new(self))
    }
}

impl callables::IntoReturnPart for TaskFlowOutput {
    fn into_return_part() -> callables::ReturnPart {
        callables::ReturnPart::Empty
    }
}

#[derive(Debug, Clone)]
pub enum TaskUnitOutput {
    Retry,
    Success(String),
}

impl callables::IntoOutput<TaskError> for TaskUnitOutput {
    fn into_output(self) -> Result<callables::PayloadData, TaskError> {
        Ok(callables::PayloadData::new(self))
    }
}

impl callables::IntoReturnPart for TaskUnitOutput {
    fn into_return_part() -> callables::ReturnPart {
        callables::ReturnPart::Empty
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskOutput {
    pub id: uuid::Uuid,
    pub task_id: uuid::Uuid,
    pub child_id: Option<uuid::Uuid>,
    pub status: TaskStatus,
    pub data: String,
    pub create_time: chrono::DateTime<chrono::Utc>,
}

impl TaskOutput {
    pub fn from_error(task_id: uuid::Uuid, error: TaskError) -> Self {
        Self {
            id: uuid::Uuid::now_v7(),
            child_id: None,
            task_id,
            status: TaskStatus::Failure,
            data: format!("Task failed with error: {}", error),
            create_time: chrono::Utc::now(),
        }
    }

    pub fn from_flow(task_id: uuid::Uuid, output: &TaskFlowOutput) -> Self {
        match output {
            TaskFlowOutput::Suspend(child_id) => Self {
                id: uuid::Uuid::now_v7(),
                child_id: Some(*child_id),
                task_id,
                status: TaskStatus::Suspend,
                data: "".to_string(),
                create_time: chrono::Utc::now(),
            },
            TaskFlowOutput::Success(data) => Self {
                id: uuid::Uuid::now_v7(),
                child_id: None,
                task_id,
                status: TaskStatus::Success,
                data: data.clone(),
                create_time: chrono::Utc::now(),
            },
        }
    }

    pub fn from_unit(task_id: uuid::Uuid, output: &TaskUnitOutput) -> Self {
        match output {
            TaskUnitOutput::Retry => Self {
                id: uuid::Uuid::now_v7(),
                child_id: None,
                task_id,
                status: TaskStatus::Ready,
                data: "".to_string(),
                create_time: chrono::Utc::now(),
            },
            TaskUnitOutput::Success(data) => Self {
                id: uuid::Uuid::now_v7(),
                child_id: None,
                task_id,
                status: TaskStatus::Success,
                data: data.clone(),
                create_time: chrono::Utc::now(),
            },
        }
    }
}


pub struct TaskMeta {
    pub about: Cow<'static, str>,
    pub type_name: Cow<'static, str>,
    pub schema_fn: fn(&mut schemars::SchemaGenerator) -> schemars::Schema,
}

#[derive(Clone)]
pub struct TaskService {
    pub name: String,
    pub type_id: TypeId,
    // pub source_type_id: TypeId,
    pub type_name: String,
    // pub handler: TaskHandler,
    pub coerce: fn(&str) -> Result<(), TaskError>,
    pub(crate) kind: TaskKind,
    handler: TaskHandler,
}

impl TaskService {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn validate_data(&self, data: &str) -> Result<(), TaskError> {
        (self.coerce)(data)
    }

    pub fn validate_object<T: 'static>(&self, _obj: &T) -> Result<(), TaskError> {
        if self.type_id != TypeId::of::<T>() {
            return Err(TaskError::TypeMismatch(
                self.type_name.clone(),
                std::any::type_name::<T>().to_string(),
            ));
        }
        Ok(())
    }

    pub async fn execute(&self, site: Site, input: Arc<TaskInput>) -> TaskOutput {
        let task_id = input.id;
        
        // Parse input data to PayloadData
        let payload = match input.to::<serde_json::Value>() {
            Ok(val) => callables::PayloadData::new(val),
            Err(e) => return TaskOutput::from_error(task_id, e),
        };
        
        let ctx = TaskContext {
            site,
            payload,
        };

        let data = match self.handler.call(ctx).await {
            Ok(data) => data,
            Err(e) => return TaskOutput::from_error(task_id, e),
        };

        match &self.kind {
            TaskKind::Flow => match data.downcast_ref::<TaskFlowOutput>() {
                Some(output) => TaskOutput::from_flow(task_id, output),
                None => TaskOutput::from_error(
                    task_id,
                    TaskError::TypeMismatch("TaskFlowOutput".to_string(), "Unknown".to_string()),
                ),
            },
            TaskKind::Unit => match data.downcast_ref::<TaskUnitOutput>() {
                Some(output) => TaskOutput::from_unit(task_id, output),
                None => TaskOutput::from_error(
                    task_id,
                    TaskError::TypeMismatch("TaskUnitOutput".to_string(), "Unknown".to_string()),
                ),
            },
        }
    }

    fn execute_flow(&self, site: Site, input: Arc<TaskInput>) -> TaskOutput {
        let task_id = input.id;
        
        if !matches!(self.kind, TaskKind::Flow) {
            return TaskOutput::from_error(task_id, TaskError::UnexpectedFlowKind);
        }
        
        // Parse input data to PayloadData
        let payload = match input.to::<serde_json::Value>() {
            Ok(val) => callables::PayloadData::new(val),
            Err(e) => return TaskOutput::from_error(task_id, e),
        };
        
        let ctx = TaskContext {
            site,
            payload,
        };
        
        // Flow tasks are executed synchronously by blocking on the async call
        let data = match tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.handler.call(ctx))
        }) {
            Ok(data) => data,
            Err(e) => return TaskOutput::from_error(task_id, e),
        };
        
        match data.downcast_ref::<TaskFlowOutput>() {
            Some(output) => TaskOutput::from_flow(task_id, output),
            None => TaskOutput::from_error(
                task_id,
                TaskError::TypeMismatch("TaskFlowOutput".to_string(), "Unknown".to_string()),
            ),
        }
    }

    pub fn from_flow<T, H, Args>(name: &str, handler: H) -> Self
    where
        T: callables::Payloadable,
        H: callables::Specable<Args, Output = TaskFlowOutput> + Send + Sync + 'static,
        Args: callables::FromContext<TaskContext>
            + callables::IntoArgSpecs
            + callables::HasPayload<T>
            + Send
            + 'static,
    {
        let callable: callables::Callable<TaskContext, TaskError> = Callable::new(handler);
        let coerce = |data: &str| -> Result<(), TaskError> {
            let _: T = serde_json::from_str(data)?;
            Ok(())
        };
        TaskService {
            name: name.to_string(),
            type_name: std::any::type_name::<T>().to_string(),
            type_id: TypeId::of::<T>(),
            kind: TaskKind::Flow,
            coerce: coerce,
            handler: callable,
        }
    }

    pub fn from_unit<T, H, Args>(name: &str, handler: H) -> Self
    where
        T: callables::Payloadable,
        H: callables::Specable<Args, Output = TaskUnitOutput> + Send + Sync + 'static,
        Args: callables::FromContext<TaskContext>
            + callables::IntoArgSpecs
            + callables::HasPayload<T>
            + Send
            + 'static,
    {
        let callable: callables::Callable<TaskContext, TaskError> = Callable::new(handler);
        let coerce = |data: &str| -> Result<(), TaskError> {
            let _: T = serde_json::from_str(data)?;
            Ok(())
        };
        TaskService {
            name: name.to_string(),
            type_id: TypeId::of::<T>(),
            kind: TaskKind::Unit,
            type_name: std::any::type_name::<T>().to_string(),
            coerce: coerce,
            handler: callable,
        }
    }
}

#[derive(Clone)]
pub struct TaskRegistry {
    pub(crate) config: TaskConf,    
    pub(crate) tasks: HashMap<String, TaskService>,
    pub(crate) typed_map: HashMap<TypeId, String>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            config: TaskConf::default(),
            tasks: HashMap::new(),
            typed_map: HashMap::new(),
        }
    }

    pub fn with_config(self, config: TaskConf) -> Self {
        Self {
            config,
            tasks: self.tasks,
            typed_map: self.typed_map,
        }
    }

    pub fn iter_services(&self) -> impl Iterator<Item = &TaskService> {
        self.tasks.values()
    }

    pub fn register(&mut self, service: TaskService) -> Result<(), TaskError> {
        let name = service.name().to_string();
        if self.tasks.contains_key(&name) || self.typed_map.contains_key(&service.type_id) {
            return Err(TaskError::AlreadyExists(name));
        }
        self.typed_map.insert(service.type_id, name.clone());
        self.tasks.insert(name, service);
        Ok(())
    }

    pub fn merge(&mut self, other: TaskRegistry) -> Result<(), TaskError> {
        for (name, task) in other.tasks {
            if self.tasks.contains_key(&name) {
                return Err(TaskError::AlreadyExists(name));
            }
            if self.typed_map.contains_key(&task.type_id) {
                return Err(TaskError::AlreadyExists(name));
            }
            self.typed_map.insert(task.type_id, name.clone());
            self.tasks.insert(name, task);
        }
        Ok(())
    }

    pub(crate) fn dispatcher<S: AbstractTaskStore + Send + Sync + 'static>(
        self: Arc<Self>,
        store: Arc<S>,
    ) -> TaskDispatcher<S> {
        TaskDispatcher {
            store: store.clone(),
            registry: self.clone(),
            notifier: Arc::new(tokio::sync::Notify::new()),
            identity_cache: IdentityCache::new(),
        }
    }

    pub async fn execute(&self, site: Site, input: Arc<TaskInput>) -> TaskOutput {
        let task = match self.tasks.get(input.name()) {
            Some(task) => task,
            None => {
                return TaskOutput::from_error(
                    input.id,
                    TaskError::TaskNotFound(input.name().to_string()),
                );
            }
        };
        task.execute(site, input).await
    }

    pub fn execute_flow(&self, site: Site, input: Arc<TaskInput>) -> TaskOutput {
        let task = match self.tasks.get(input.name()) {
            Some(task) => task,
            None => {
                return TaskOutput::from_error(
                    input.id,
                    TaskError::TaskNotFound(input.name().to_string()),
                );
            }
        };
        task.execute_flow(site, input)
    }
}

#[derive(Clone)]
pub struct TaskDispatcher<S: AbstractTaskStore + Send + Sync + 'static> {
    pub(crate) store: Arc<S>,
    pub(crate) notifier: Arc<tokio::sync::Notify>,
    pub(crate) registry: Arc<TaskRegistry>,
    identity_cache: IdentityCache,
}

impl<S: AbstractTaskStore + Send + Sync + 'static> TaskDispatcher<S> {
    pub fn store(&self) -> Arc<S> {
        self.store.clone()
    }

    /// Submit a task by source function type with typed payload
    pub async fn submit_typed<T: 'static + Serialize>(
        &self,
        payload: T,
    ) -> Result<uuid::Uuid, TaskError> {
        let name = self
            .registry
            .typed_map
            .get(&TypeId::of::<T>())
            .ok_or_else(|| TaskError::TaskNotFound("Unknown task type".to_string()))?;
        let name = name.clone();
        self.submit::<T>(&name, payload).await
    }

    /// Submit a task by name with raw JSON data
    pub async fn submit_data(&self, name: &str, payload: &str) -> Result<uuid::Uuid, TaskError> {
        if let Some(s) = self.registry.tasks.get(name) {
            s.validate_data(&payload)?;
        } else {
            return Err(TaskError::TaskNotFound(name.to_string()));
        }
        self.submit_input_with_conf(name, payload.to_string(), TaskInputConf::default()).await
    }    

    /// Submit a task by name with typed payload
    pub async fn submit<T: 'static + Serialize>(
        &self,
        name: &str,
        payload: T,
    ) -> Result<uuid::Uuid, TaskError> {
        if let Some(s) = self.registry.tasks.get(name) {
            s.validate_object(&payload)?;
        } else {
            return Err(TaskError::TaskNotFound(name.to_string()));
        }
        let data = serde_json::to_string(&payload)?;
        self.submit_input_with_conf(name, data, TaskInputConf::default()).await
    }

    async fn submit_input_with_conf(&self, name: &str, data: String, conf: TaskInputConf) -> Result<uuid::Uuid, TaskError> {
        let ready_time = if let Some(delay) = conf.delay_ms {
            chrono::Utc::now() + chrono::Duration::milliseconds(delay as i64)
        } else {
            chrono::Utc::now()
        };
        if let Some(identity) = &conf.identity {
            if !self.identity_cache.insert(identity, Some(std::time::Duration::from_secs(3600))) {
                return Err(TaskError::IdentityError);
            }
        }
        let input = TaskInput {
            id: uuid::Uuid::now_v7(),
            root_id: uuid::Uuid::now_v7(),
            name: name.to_string(),
            child_id: None,
            data,
            state: None,
            identity: conf.identity,
            ready_time,
            status: TaskStatus::Ready,
        };
        let task_id = input.id;
        self.store.store_task(input).await?;
        self.notifier.notify_one();
        Ok(task_id)
    }
}

impl std::fmt::Debug for TaskRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskEngine")
            .field("tasks", &self.tasks.keys().collect::<Vec<_>>())
            .finish()
    }
}

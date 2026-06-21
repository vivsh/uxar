use serde::{Deserialize, Serialize};
use std::{any::TypeId, borrow::Cow, collections::HashMap, sync::Arc, time::Duration};

use crate::{
    Error, Site,
    callables::{self, Callable},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConf {
    pub poll_interval_ms: u32,
    pub capacity: usize,
    pub concurrency: usize,
    pub batch_size: usize,
    pub lease_duration_ms: u32,
}

impl Default for TaskConf {
    fn default() -> Self {
        Self {
            poll_interval_ms: 15000,
            capacity: 1000,
            concurrency: 10,
            batch_size: 250,
            lease_duration_ms: 300000,
        }
    }
}

#[derive(Clone)]
pub struct TaskContext {
    site: Site,
    payload: callables::DataBox,
    record: Arc<TaskRecord>,
}

impl callables::IntoDataBox for TaskContext {
    fn into_data_box(self) -> callables::DataBox {
        self.payload
    }
}

impl callables::HasSite for TaskContext {
    fn site(&self) -> &Site {
        &self.site
    }
}

type TaskHandler = Callable<TaskContext, Error>;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Type mismatch: expected {0}, got {1}")]
    TypeMismatch(String, String),

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

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Unknown task error: {0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[repr(i16)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending = 0,
    Running = 1,
    Suspended = 2,
    Succeeded = 3,
    Failed = 4,
}

impl TaskStatus {
    pub const fn as_i16(self) -> i16 {
        self as i16
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Running => "running",
            TaskStatus::Suspended => "suspended",
            TaskStatus::Succeeded => "succeeded",
            TaskStatus::Failed => "failed",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TaskHandlerConf {
    pub name: String,
}

impl TaskHandlerConf {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Debug, Default, Clone)]
pub struct TaskOptions {
    pub initial_delay: Option<Duration>,
    pub retry_delay: Option<Duration>,
    pub lease_duration: Option<Duration>,
    pub identity: Option<String>,
    pub max_attempts: Option<i32>,
    pub state: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskRecord {
    pub id: uuid::Uuid,
    pub name: String,
    pub input: String,
    pub state: Option<String>,
    pub resume_topic: Option<String>,
    pub resume_input: Option<String>,
    pub output: Option<String>,
    pub result: Option<String>,
    pub status: TaskStatus,
    pub attempts: i32,
    pub max_attempts: Option<i32>,
    pub retry_delay_ms: Option<i64>,
    pub lease_duration_ms: Option<i64>,
    pub last_error: Option<String>,
    pub identity: Option<String>,
    pub locked_by: Option<String>,
    pub leased_until: Option<chrono::DateTime<chrono::Utc>>,
    pub ready_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl TaskRecord {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn input<T>(&self) -> Result<T, TaskError>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_str(&self.input).map_err(TaskError::from)
    }

    pub fn state<T>(&self) -> Result<Option<T>, TaskError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.state
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(TaskError::from)
    }

    pub fn resume_input<T>(&self) -> Result<Option<T>, TaskError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.resume_input
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(TaskError::from)
    }
}

#[derive(Debug, Clone)]
pub enum TaskOutcome {
    Complete {
        result: String,
    },
    Suspend {
        topic: String,
        state: String,
        output: Option<String>,
    },
    Sleep {
        state: String,
        delay: Duration,
    },
    Retry {
        delay: Option<Duration>,
        error: String,
    },
    Fail {
        error: String,
    },
}

impl TaskOutcome {
    pub fn complete<T: Serialize>(result: &T) -> Result<Self, TaskError> {
        Ok(Self::Complete {
            result: serde_json::to_string(result)?,
        })
    }

    pub fn suspend<S: Serialize, O: Serialize>(
        topic: impl Into<String>,
        state: &S,
        output: Option<&O>,
    ) -> Result<Self, TaskError> {
        Ok(Self::Suspend {
            topic: topic.into(),
            state: serde_json::to_string(state)?,
            output: output.map(serde_json::to_string).transpose()?,
        })
    }

    pub fn sleep<S: Serialize>(state: &S, delay: Duration) -> Result<Self, TaskError> {
        Ok(Self::Sleep {
            state: serde_json::to_string(state)?,
            delay,
        })
    }

    pub fn retry(delay: Option<Duration>, error: impl Into<String>) -> Self {
        Self::Retry {
            delay,
            error: error.into(),
        }
    }

    pub fn fail(error: impl Into<String>) -> Self {
        Self::Fail {
            error: error.into(),
        }
    }

    pub fn retry_error(delay: Option<Duration>, error: &Error) -> Self {
        Self::retry(delay, error.display_compact())
    }

    pub fn fail_error(error: &Error) -> Self {
        Self::fail(error.display_compact())
    }
}

impl<E> callables::IntoOutput<E> for TaskOutcome {
    fn into_output(self) -> Result<callables::DataBox, E> {
        Ok(callables::DataBox::new(self))
    }
}

impl callables::IntoReturnPart for TaskOutcome {
    fn into_return_part() -> callables::ReturnPart {
        callables::ReturnPart::Empty
    }
}

pub struct TaskState<T>(pub Option<T>);

impl<T> callables::FromContextParts<TaskContext> for TaskState<T>
where
    T: serde::de::DeserializeOwned + Send,
{
    fn from_context_parts(ctx: &TaskContext) -> Result<Self, callables::CallError> {
        let state = ctx
            .record
            .state
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| callables::CallError::DeserializeFailed)?;
        Ok(Self(state))
    }
}

impl<T> callables::IntoArgPart for TaskState<T> {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
    }
}

pub struct TaskResume<T>(pub Option<T>);

impl<T> callables::FromContextParts<TaskContext> for TaskResume<T>
where
    T: serde::de::DeserializeOwned + Send,
{
    fn from_context_parts(ctx: &TaskContext) -> Result<Self, callables::CallError> {
        let input = ctx
            .record
            .resume_input
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| callables::CallError::DeserializeFailed)?;
        Ok(Self(input))
    }
}

impl<T> callables::IntoArgPart for TaskResume<T> {
    fn into_arg_part() -> callables::ArgPart {
        callables::ArgPart::Ignore
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
    pub type_name: String,
    pub coerce: fn(&str) -> Result<(), TaskError>,
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

    pub async fn execute(&self, site: Site, record: Arc<TaskRecord>) -> TaskOutcome {
        let payload = match self.handler.deserialize_input(&record.input) {
            Ok(value) => value,
            Err(e) => return TaskOutcome::fail(format!("Task input error: {}", e)),
        };

        let ctx = TaskContext {
            site,
            payload,
            record,
        };

        let data = match self.handler.call(ctx).await {
            Ok(data) => data,
            Err(e) => return TaskOutcome::fail_error(&e),
        };

        match data.downcast_ref::<TaskOutcome>() {
            Some(output) => output.clone(),
            None => TaskOutcome::fail("Task handler returned an unexpected output type"),
        }
    }

    pub fn new<T, H, Args>(name: &str, handler: H) -> Self
    where
        T: callables::DataValue,
        H: callables::Specable<Args> + Send + Sync + 'static,
        H::Output: callables::IntoOutput<Error> + callables::IntoReturnPart + Send + 'static,
        Args: callables::FromContext<TaskContext>
            + callables::IntoArgSpecs
            + callables::HasData<T>
            + Send
            + 'static,
    {
        let callable: callables::Callable<TaskContext, Error> = Callable::new(handler);
        let coerce = |data: &str| -> Result<(), TaskError> {
            let _: T = serde_json::from_str(data)?;
            Ok(())
        };
        TaskService {
            name: name.to_string(),
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>().to_string(),
            coerce,
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

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
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

    pub(crate) fn dispatcher<S: crate::tasks::store::AbstractTaskStore + Send + Sync + 'static>(
        self: Arc<Self>,
        store: Arc<S>,
    ) -> TaskDispatcher<S> {
        TaskDispatcher {
            store,
            registry: self.clone(),
            notifier: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn execute(&self, site: Site, record: Arc<TaskRecord>) -> TaskOutcome {
        let task = match self.tasks.get(record.name()) {
            Some(task) => task,
            None => return TaskOutcome::fail(format!("Task '{}' not found", record.name())),
        };
        task.execute(site, record).await
    }
}

#[derive(Clone)]
pub struct TaskDispatcher<S: crate::tasks::store::AbstractTaskStore + Send + Sync + 'static> {
    pub(crate) store: Arc<S>,
    pub(crate) notifier: Arc<tokio::sync::Notify>,
    pub(crate) registry: Arc<TaskRegistry>,
}

#[derive(Clone)]
pub struct TaskClient<S: crate::tasks::store::AbstractTaskStore + Send + Sync + 'static> {
    dispatcher: TaskDispatcher<S>,
}

impl<S: crate::tasks::store::AbstractTaskStore + Send + Sync + 'static> TaskClient<S> {
    pub(crate) fn new(dispatcher: TaskDispatcher<S>) -> Self {
        Self { dispatcher }
    }

    pub async fn submit<T: Serialize + 'static>(&self, input: T) -> Result<uuid::Uuid, TaskError> {
        self.dispatcher.submit(input).await
    }

    pub async fn submit_with<T: Serialize + 'static>(
        &self,
        input: T,
        conf: TaskOptions,
    ) -> Result<uuid::Uuid, TaskError> {
        self.dispatcher.submit_with(input, conf).await
    }

    pub async fn resume<T: Serialize>(&self, topic: &str, input: T) -> Result<u64, TaskError> {
        self.dispatcher.resume(topic, input).await
    }
}

impl<S: crate::tasks::store::AbstractTaskStore + Send + Sync + 'static> TaskDispatcher<S> {
    pub fn has_tasks(&self) -> bool {
        !self.registry.is_empty()
    }

    pub fn store(&self) -> Arc<S> {
        self.store.clone()
    }

    pub async fn submit<T: 'static + Serialize>(&self, input: T) -> Result<uuid::Uuid, TaskError> {
        let name = self
            .registry
            .typed_map
            .get(&TypeId::of::<T>())
            .ok_or_else(|| TaskError::TaskNotFound("Unknown task type".to_string()))?
            .clone();
        self.submit_registered::<T>(&name, input, TaskOptions::default())
            .await
    }

    pub async fn submit_with<T: 'static + Serialize>(
        &self,
        input: T,
        conf: TaskOptions,
    ) -> Result<uuid::Uuid, TaskError> {
        let name = self
            .registry
            .typed_map
            .get(&TypeId::of::<T>())
            .ok_or_else(|| TaskError::TaskNotFound("Unknown task type".to_string()))?
            .clone();
        self.submit_registered::<T>(&name, input, conf).await
    }

    async fn submit_registered<T: 'static + Serialize>(
        &self,
        name: &str,
        input: T,
        conf: TaskOptions,
    ) -> Result<uuid::Uuid, TaskError> {
        if let Some(s) = self.registry.tasks.get(name) {
            s.validate_object(&input)?;
        } else {
            return Err(TaskError::TaskNotFound(name.to_string()));
        }
        let data = serde_json::to_string(&input)?;
        self.submit_serialized(name, data, conf).await
    }

    async fn submit_serialized(
        &self,
        name: &str,
        input: String,
        conf: TaskOptions,
    ) -> Result<uuid::Uuid, TaskError> {
        let now = chrono::Utc::now();
        let ready_at = Some(match conf.initial_delay {
            Some(delay) => now + chrono::Duration::from_std(delay).unwrap_or_default(),
            None => now,
        });
        let retry_delay_ms = conf
            .retry_delay
            .map(|delay| delay.as_millis().min(i64::MAX as u128) as i64);
        let lease_duration_ms = conf
            .lease_duration
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64);
        let record = TaskRecord {
            id: uuid::Uuid::now_v7(),
            name: name.to_string(),
            input,
            state: conf.state,
            resume_topic: None,
            resume_input: None,
            output: None,
            result: None,
            status: TaskStatus::Pending,
            attempts: 0,
            max_attempts: conf.max_attempts,
            retry_delay_ms,
            lease_duration_ms,
            last_error: None,
            identity: conf.identity,
            locked_by: None,
            leased_until: None,
            ready_at,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        let task_id = record.id;
        self.store.store_task(record).await?;
        self.notifier.notify_one();
        Ok(task_id)
    }

    pub async fn resume<T: Serialize>(&self, topic: &str, input: T) -> Result<u64, TaskError> {
        let input = serde_json::to_string(&input)?;
        let count = self.store.resume(topic, input).await?;
        if count > 0 {
            self.notifier.notify_waiters();
        }
        Ok(count)
    }
}

impl std::fmt::Debug for TaskRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskRegistry")
            .field("tasks", &self.tasks.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{
        Data,
        tasks::{MemoryTaskStore, store::AbstractTaskStore},
    };

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    struct DirectJob {
        id: i64,
    }

    async fn direct_job(input: Data<DirectJob>) -> TaskOutcome {
        TaskOutcome::complete(&format!("direct:{}", input.id)).unwrap()
    }

    #[tokio::test]
    async fn direct_registration_supports_typed_submit() -> Result<(), TaskError> {
        let mut registry = TaskRegistry::new();
        registry.register(TaskService::new("direct_job", direct_job))?;

        let store = Arc::new(MemoryTaskStore::new(10));
        let dispatcher = Arc::new(registry).dispatcher(store.clone());
        let client = TaskClient::new(dispatcher);

        let task_id = client.submit(DirectJob { id: 42 }).await?;
        let claimed = store.claim_tasks("runner-a").await?;

        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].id, task_id);
        assert_eq!(claimed[0].name, "direct_job");
        assert_eq!(claimed[0].input::<DirectJob>()?.id, 42);

        store
            .commit_outcome(task_id, "runner-a", TaskOutcome::complete(&"done")?)
            .await?;
        Ok(())
    }
}

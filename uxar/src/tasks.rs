use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::future::Future;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::Site;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Failed to deserialize task arguments: {0}")]
    DeserializationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Task engine build error: {0}")]
    BuildError(String),

    #[error("Too many scheduled tasks. Exceeded limit of {0}")]
    TooManyScheduledTasks(usize),

    #[error("Failed to invoke task: {0}")]
    InvokeError(String),

    #[error("Invalid interval (millis): {0}")]
    InvalidInterval(i64),

    #[error("Duplicate task name: {0}")]
    DuplicateTaskName(String),
}

/// Type-erased handler trait for internal use
trait TaskHandlerErased: Send + Sync {
    fn spawn_call(&self, site: Site, args: Value);
}

/// Wrapper that handles typed args -> Value conversion internally
#[derive(Debug)]
struct TypedHandler<T, F> {
    handler: F,
    _phantom: PhantomData<T>,
}

impl<T, F, Fut> TaskHandlerErased for TypedHandler<T, F>
where
    T: DeserializeOwned + Send + Sync + 'static,
    F: Fn(Site, T) -> Fut + Send + Sync,
    Fut: Future<Output = ()> + Send + 'static,
{
    fn spawn_call(&self, site: Site, args: Value) {
        match serde_json::from_value::<T>(args) {
            Ok(typed_args) => {
                let fut = (self.handler)(site, typed_args);
                tokio::spawn(fut);
            }
            Err(e) => {
                tracing::error!("Failed to deserialize task arguments: {}", e);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Oneshot,
    Cron(Arc<Schedule>),
    Interval(i64), // Milliseconds
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: Uuid,
    pub handler: usize,
    pub kind: TaskKind,
    pub input: Value,
    pub next_time: DateTime<Utc>,
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.next_time == other.next_time && self.id == other.id
    }
}

impl Eq for Task {}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .next_time
            .cmp(&self.next_time)
            .then_with(|| other.id.cmp(&self.id))
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


struct TaskRegistry {
    handlers: Vec<Arc<dyn TaskHandlerErased>>,
    name_to_index: HashMap<String, usize>,
    timezone: Tz,
}

impl TaskRegistry {
    fn new(timezone: Tz) -> Self {
        Self {
            handlers: Vec::new(),
            name_to_index: HashMap::new(),
            timezone,
        }
    }

    fn register<T, F, Fut>(&mut self, name: &str, handler: F) -> Result<usize, TaskError>
    where
        T: DeserializeOwned + Send + Sync + 'static,
        F: Fn(Site, T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.name_to_index.contains_key(name) {
            return Err(TaskError::DuplicateTaskName(name.to_string()));
        }

        let index = self.handlers.len();
        self.handlers.push(Arc::new(TypedHandler {
            handler,
            _phantom: PhantomData,
        }));
        self.name_to_index.insert(name.to_string(), index);
        Ok(index)
    }

    fn get_index(&self, name: &str) -> Option<usize> {
        self.name_to_index.get(name).copied()
    }

    fn get_handler(&self, index: usize) -> Option<&Arc<dyn TaskHandlerErased>> {
        self.handlers.get(index)
    }

    fn timezone(&self) -> Tz {
        self.timezone
    }
}

struct ScheduledTask {
    task_name: String,
    kind: TaskKind,
    input: Value,
    cron_expr: Option<String>,
}

/// Build-time task scheduler (used in SiteBuilder)
pub(crate) struct TaskEngineBuilder {
    registry: TaskRegistry,
    scheduled_tasks: Vec<ScheduledTask>,
    build_errors: Vec<TaskError>,
}

impl TaskEngineBuilder {
    pub fn new(timezone: Tz) -> Self {
        Self {
            registry: TaskRegistry::new(timezone),
            scheduled_tasks: Vec::new(),
            build_errors: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.registry = TaskRegistry::new(self.registry.timezone());
        self.scheduled_tasks.clear();
        self.build_errors.clear();
    }

    pub fn register<T, F, Fut>(&mut self, name: &str, handler: F)
    where
        T: DeserializeOwned + Send + Sync + 'static,
        F: Fn(Site, T) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if let Err(err) = self.registry.register(name, handler) {
            self.build_errors.push(err);
        }
    }

    pub fn schedule_cron<T: Serialize>(
        &mut self,
        name: &str,
        cron_expr: &str,
        arg: T,
    ) {
        let schedule = match Schedule::from_str(cron_expr)
            .map_err(|e| TaskError::BuildError(format!("Invalid cron expression: {}", e)))
        {
            Ok(sched) => sched,
            Err(err) => {
                self.build_errors.push(err);
                return;
            }
        };

        let input = match serde_json::to_value(arg) {
            Ok(value) => value,
            Err(e) => {
                self.build_errors.push(TaskError::DeserializationError(e.to_string()));
                return;
            }
        };

        self.scheduled_tasks.push(ScheduledTask {
            task_name: name.to_string(),
            kind: TaskKind::Cron(Arc::new(schedule)),
            input,
            cron_expr: Some(cron_expr.to_string()),
        });
    }

    pub fn schedule_interval<T: Serialize>(
        &mut self,
        name: &str,
        millis: i64,
        arg: T,
    ){
        if millis <= 0 {
            self.build_errors.push(TaskError::InvalidInterval(millis));
            return;
        }
        let input = match serde_json::to_value(arg) {
            Ok(value) => value,
            Err(e) => {
                self.build_errors.push(TaskError::DeserializationError(e.to_string()));
                return;
            }
        };

        self.scheduled_tasks.push(ScheduledTask {
            task_name: name.to_string(),
            kind: TaskKind::Interval(millis),
            input,
            cron_expr: None,
        });

    }

    pub fn build(self) -> Result<TaskEngine, TaskError> {
        let tz = self.registry.timezone();
        self.build_tz(tz)
    }

    pub fn build_tz(mut self, timezone: Tz) -> Result<TaskEngine, TaskError> {
        if let Some(err) = self.build_errors.into_iter().next() {
            return Err(err);
        }

        let scheduled_tasks = std::mem::take(&mut self.scheduled_tasks);
        self.registry.timezone = timezone;
        let registry = Arc::new(self.registry);
        let engine = TaskEngine::new(Arc::clone(&registry));

        enqueue_scheduled_tasks(&engine, &registry, scheduled_tasks)?;
        Ok(engine)
    }

    fn build_scheduled_task(
        registry: &TaskRegistry,
        scheduled: ScheduledTask,
    ) -> Result<Task, TaskError> {
        let handler_index = registry
            .get_index(&scheduled.task_name)
            .ok_or_else(|| TaskError::TaskNotFound(scheduled.task_name.clone()))?;

        let next_time = scheduled_next_time(&scheduled, registry.timezone())?;

        Ok(Task {
            id: Uuid::now_v7(),
            handler: handler_index,
            kind: scheduled.kind,
            input: scheduled.input,
            next_time,
        })
    }
}

fn enqueue_scheduled_tasks(
    engine: &TaskEngine,
    registry: &TaskRegistry,
    scheduled_tasks: Vec<ScheduledTask>,
) -> Result<(), TaskError> {
    for scheduled in scheduled_tasks {
        let task = TaskEngineBuilder::build_scheduled_task(registry, scheduled)?;
        engine.try_enqueue(task)?;
    }
    Ok(())
}

fn scheduled_next_time(scheduled: &ScheduledTask, tz: Tz) -> Result<DateTime<Utc>, TaskError> {
    match &scheduled.kind {
        TaskKind::Cron(sched) => {
            let next = sched.upcoming(tz).next().ok_or_else(|| {
                let cron_expr = scheduled.cron_expr.as_deref().unwrap_or("<unknown>");
                TaskError::BuildError(format!(
                    "Cron schedule has no upcoming times (task='{}', cron='{}', tz='{}')",
                    scheduled.task_name, cron_expr, tz
                ))
            })?;
            Ok(next.with_timezone(&Utc))
        }
        TaskKind::Interval(millis) => Ok(Utc::now() + chrono::Duration::milliseconds(*millis)),
        TaskKind::Oneshot => Ok(Utc::now()),
    }
}

/// Runtime handle for one-shot task invocation only (immutable registry)
/// Should be stored inside Site
#[derive(Clone)]
pub struct TaskEngine {
    sender: mpsc::Sender<Task>,
    receiver: Arc<parking_lot::Mutex<Option<mpsc::Receiver<Task>>>>,
    registry: Arc<TaskRegistry>,
}


// Dummy Debug impl meant only to satisfy the siteinner's weird constraints
impl std::fmt::Debug for TaskEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TaskEngine {{ sender: ..., receiver: ..., registry: ... }}")
    }
}

impl TaskEngine {

    fn new(registry: Arc<TaskRegistry>) -> Self {
        let (sender, receiver) = mpsc::channel(4096);
        Self { 
            sender, 
            receiver: Arc::new(parking_lot::Mutex::new(Some(receiver))), 
            registry 
        }
    }

    fn try_enqueue(&self, task: Task) -> Result<(), TaskError> {
        self.sender
            .try_send(task)
            .map_err(|_| TaskError::TooManyScheduledTasks(4096))
    }

    pub(crate) async fn start_runner(&self, site: Site, shutdown: Arc<tokio::sync::Notify>) -> Result<(), TaskError> {
        let receiver = self.receiver.lock().take()
            .ok_or_else(|| TaskError::BuildError("Task runner already started".to_string()))?;
        let registry = Arc::clone(&self.registry);
        let runner = TaskRunner::new(registry, receiver);

        tokio::spawn(async move {
            runner.run(shutdown, site.clone()).await;
        });

        Ok(())
    }

    pub fn manager<'a>(&'a self) -> TaskManager<'a> {
        TaskManager::new(self)
    }

    async fn invoke<T: Serialize>(&self, name: &str, input: T) -> Result<(), TaskError> {
        let at = Utc::now();
        self.invoke_at(name, input, at).await
    }

    /// Invoke a one-shot task by name with typed arguments
    /// To be called from Site methods
    async fn invoke_at<T: Serialize>(&self, name: &str, input: T, at: chrono::DateTime<Utc>) -> Result<(), TaskError> {
        let handler_index = self
            .registry
            .get_index(name)
            .ok_or_else(|| TaskError::TaskNotFound(name.to_string()))?;

        let input = serde_json::to_value(input)
            .map_err(|e| TaskError::DeserializationError(e.to_string()))?;

        let task = Task {
            id: Uuid::now_v7(),
            handler: handler_index,
            kind: TaskKind::Oneshot,
            input,
            next_time: at,
        };

        self.sender
            .send(task)
            .await
            .map_err(|_| TaskError::InvokeError(format!("Task '{}' could not be queued", name)))
    }
}


/// The public API for scheduling and running tasks
pub struct TaskManager<'a>{
    handle: &'a TaskEngine,
}

impl TaskManager<'_>{

    fn new(handle: &TaskEngine) -> TaskManager<'_> {
        TaskManager { handle }
    }

    pub async fn run<T: Serialize>(&self, task_name: &str, args: T) -> Result<(), TaskError> {
        let at = Utc::now();
        self.handle.invoke_at(task_name, args, at).await
    }

    pub async fn run_later<T: Serialize>(&self, task_name: &str, args: T, delay_millis: i64) -> Result<(), TaskError> {
        let at = Utc::now() + chrono::Duration::milliseconds(delay_millis);
        self.handle.invoke_at(task_name, args, at).await
    }

    pub async fn run_at<T: Serialize>(&self, task_name: &str, args: T, at: chrono::DateTime<Utc>) -> Result<(), TaskError> {
        self.handle.invoke_at(task_name, args, at).await
    }

}

/// Background runner that executes tasks
struct TaskRunner {
    registry: Arc<TaskRegistry>,
    receiver: mpsc::Receiver<Task>,
    queue: BinaryHeap<Task>,
}

impl TaskRunner {
    fn new(registry: Arc<TaskRegistry>, receiver: mpsc::Receiver<Task>) -> Self {
        Self {
            registry,
            receiver,
            queue: BinaryHeap::new(),
        }
    }

    async fn run(mut self, shutdown: Arc<tokio::sync::Notify>, site: Site) {
        while self.tick(&shutdown, &site).await {}
    }

    async fn tick(&mut self, shutdown: &Arc<tokio::sync::Notify>, site: &Site) -> bool {
        let sleep_until = self.sleep_until();
        let queue_empty = self.queue.is_empty();

        tokio::select! {
            _ = shutdown.notified() => {
                tracing::info!("Task runner shutting down");
                return false;
            }
            Some(task) = self.receiver.recv() => {
                self.queue.push(task);
            }
            _ = Self::wait(sleep_until, queue_empty) => {}
        }

        self.drain_due(site).await;
        true
    }

    fn sleep_until(&self) -> Option<DateTime<Utc>> {
        let now = Utc::now();
        self.queue.peek().and_then(|task| {
            if task.next_time <= now {
                None
            } else {
                Some(task.next_time)
            }
        })
    }

    async fn wait(sleep_until: Option<DateTime<Utc>>, queue_empty: bool) {
        match sleep_until {
            Some(time) => {
                let duration = (time - Utc::now())
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO);
                tokio::time::sleep(duration).await;
            }
            None if queue_empty => {
                std::future::pending::<()>().await;
            }
            None => {}
        }
    }

    async fn drain_due(&mut self, site: &Site) {
        loop {
            let due = self.queue.peek().is_some_and(|task| task.next_time <= Utc::now());
            if !due {
                break;
            }
            if let Some(task) = self.queue.pop() {
                self.process_task(task, site).await;
            } else {
                break;
            }
        }
    }

    async fn process_task(&mut self, task: Task, site: &Site) {
        if let Some(handler) = self.registry.get_handler(task.handler) {
            handler.spawn_call(site.clone(), task.input.clone());
        } else {
            tracing::warn!("Task handler index {} not found", task.handler);
        }

        self.reschedule(task);
    }

    fn reschedule(&mut self, mut task: Task) {
        match &task.kind {
            TaskKind::Oneshot => {}
            TaskKind::Interval(millis) => {
                if *millis <= 0 {
                    tracing::error!(
                        "Invalid interval millis={} for task id={}",
                        millis,
                        task.id
                    );
                    return;
                }
                task.next_time += chrono::Duration::milliseconds(*millis);
                self.queue.push(task);
            }
            TaskKind::Cron(schedule) => {
                let tz = self.registry.timezone();
                let scheduled_time = task.next_time.with_timezone(&tz);

                if let Some(next) = schedule.after(&scheduled_time).next() {
                    task.next_time = next.with_timezone(&Utc);
                    self.queue.push(task);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use chrono_tz::America::New_York;
    use serde::{Deserialize as De};

    #[tokio::test]
    async fn test_register_and_invoke_basic_task() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("increment", move |_site, _: ()| {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        engine.manager().run_at("increment", (), Utc::now()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        assert_eq!(counter.load(AtomicOrdering::SeqCst), 1);
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_typed_handler_arguments() {
        #[derive(Debug, Serialize, De)]
        struct TaskArgs {
            value: i32,
            message: String,
        }

        let result = Arc::new(parking_lot::Mutex::new(None));
        let result_clone = result.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<TaskArgs, _, _>("process", move |_site, args: TaskArgs| {
            let result = result_clone.clone();
            async move {
                *result.lock() = Some(format!("{}: {}", args.message, args.value));
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        let args = TaskArgs {
            value: 42,
            message: "Answer".to_string(),
        };
        engine.manager().run_at("process", args, Utc::now()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        let result_value = result.lock().clone();
        assert_eq!(result_value, Some("Answer: 42".to_string()));
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_task_not_found_error() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("exists", |_site, _: ()| async {});

        let engine = builder.build().unwrap();

        let result = engine.manager().run_at("nonexistent", (), Utc::now()).await;
        assert!(matches!(result, Err(TaskError::TaskNotFound(_))));
        
        if let Err(TaskError::TaskNotFound(name)) = result {
            assert_eq!(name, "nonexistent");
        }
    }

    #[tokio::test]
    async fn test_schedule_cron_not_found() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        
        builder.schedule_cron("nonexistent", "0 0 0 * * *", ());
        let result = builder.build();
        assert!(matches!(result, Err(TaskError::TaskNotFound(_))));
    }

    #[tokio::test]
    async fn test_schedule_interval_not_found() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        
        builder.schedule_interval("nonexistent", 1000, ());
        let result = builder.build();
        assert!(matches!(result, Err(TaskError::TaskNotFound(_))), "got {:?}", result);
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("task", |_site, _: ()| async {});

        builder.schedule_cron("task", "invalid cron", ());
        let result = builder.build();
        assert!(result.is_err());
        
        if let Err(TaskError::BuildError(msg)) = result {
            assert!(msg.contains("Invalid cron expression"));
        }
    }

    #[tokio::test]
    async fn test_cron_no_upcoming_is_error() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("past", |_site, _: ()| async {});

        // A cron expression limited to a past year should yield no upcoming times.
        builder
            .schedule_cron("past", "0 0 0 1 1 * 1970", ());

        let result = builder.build();
        assert!(matches!(result, Err(TaskError::BuildError(_))));
        if let Err(TaskError::BuildError(msg)) = result {
            assert!(msg.contains("no upcoming times"), "unexpected msg: {msg}");
        }
    }

    #[tokio::test]
    async fn test_schedule_cron_success() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("daily", |_site, _: ()| async {});

        builder.schedule_cron("daily", "0 0 0 * * *", ());
        assert!(builder.build().is_ok());
    }

    #[tokio::test]
    async fn test_schedule_interval_success() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("frequent", |_site, _: ()| async {});

        builder.schedule_interval("frequent", 5000, ());
        assert!(builder.build().is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_task_name_is_error() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("dup", |_site, _: ()| async {});

        builder.register::<(), _, _>("dup", |_site, _: ()| async {});
        let result = builder.build();
        assert!(matches!(result, Err(TaskError::DuplicateTaskName(_))));
    }

    #[tokio::test]
    async fn test_double_start_runner_fails() {
        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("task", |_site, _: ()| async {});

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        let first = engine.start_runner(site.clone(), shutdown.clone()).await;
        assert!(first.is_ok());

        let second = engine.start_runner(site.clone(), shutdown.clone()).await;
        assert!(matches!(second, Err(TaskError::BuildError(_))));
        
        if let Err(TaskError::BuildError(msg)) = second {
            assert_eq!(msg, "Task runner already started");
        }
        
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_multiple_handlers() {
        let counter_a = Arc::new(AtomicUsize::new(0));
        let counter_b = Arc::new(AtomicUsize::new(0));
        let counter_a_clone = counter_a.clone();
        let counter_b_clone = counter_b.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("task_a", move |_site, _: ()| {
            let counter = counter_a_clone.clone();
            async move {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });
        builder.register::<(), _, _>("task_b", move |_site, _: ()| {
            let counter = counter_b_clone.clone();
            async move {
                counter.fetch_add(10, AtomicOrdering::SeqCst);
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        engine.manager().run_at("task_a", (), Utc::now()).await.unwrap();
        engine.invoke("task_b", ()).await.unwrap();
        engine.invoke("task_a", ()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        
        assert_eq!(counter_a.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(counter_b.load(AtomicOrdering::SeqCst), 10);
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_timezone_aware_scheduling() {
        let mut builder = TaskEngineBuilder::new(New_York);
        builder.register::<(), _, _>("tz_task", |_site, _: ()| async {});

        builder.schedule_cron("tz_task", "0 0 0 * * *", ());

        let engine = builder.build();

        assert!(engine.is_ok());

        let engine = engine.unwrap();
        
        assert!(engine.registry.timezone() == New_York);
    }

    #[tokio::test]
    async fn test_deserialization_error_handling() {
        #[derive(Debug, Serialize, De)]
        struct RequiredArgs {
            required_field: String,
        }

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<RequiredArgs, _, _>("strict", |_site, _args: RequiredArgs| async {});

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        let result = engine.invoke("strict", 42).await;
        assert!(result.is_ok());
        
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_task_ordering_in_heap() {
        let task1 = Task {
            id: Uuid::now_v7(),
            handler: 0,
            kind: TaskKind::Oneshot,
            input: Value::Null,
            next_time: Utc::now() + chrono::Duration::seconds(10),
        };
        
        let task2 = Task {
            id: Uuid::now_v7(),
            handler: 0,
            kind: TaskKind::Oneshot,
            input: Value::Null,
            next_time: Utc::now() + chrono::Duration::seconds(5),
        };

        let mut heap = BinaryHeap::new();
        heap.push(task1.clone());
        heap.push(task2.clone());

        let first = heap.pop().unwrap();
        assert_eq!(first.next_time, task2.next_time);
        
        let second = heap.pop().unwrap();
        assert_eq!(second.next_time, task1.next_time);
    }

    #[tokio::test]
    async fn test_empty_arguments() {
        let called = Arc::new(AtomicUsize::new(0));
        let called_clone = called.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("empty", move |_site, _: ()| {
            let called = called_clone.clone();
            async move {
                called.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        engine.invoke("empty", ()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        assert_eq!(called.load(AtomicOrdering::SeqCst), 1);
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_complex_nested_arguments() {
        use std::collections::HashMap;

        #[derive(Debug, Clone, Serialize, De, PartialEq)]
        struct NestedArgs {
            items: Vec<String>,
            metadata: HashMap<String, i32>,
        }

        let result = Arc::new(parking_lot::Mutex::new(None));
        let result_clone = result.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<NestedArgs, _, _>("nested", move |_site, args: NestedArgs| {
            let result = result_clone.clone();
            async move {
                *result.lock() = Some(args);
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        let mut metadata = HashMap::new();
        metadata.insert("count".to_string(), 5);
        metadata.insert("priority".to_string(), 10);
        
        let args = NestedArgs {
            items: vec!["a".to_string(), "b".to_string()],
            metadata,
        };
        let args_expected = args.clone();
        
        engine.invoke("nested", args).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        let result_value = result.lock().clone();
        assert_eq!(result_value, Some(args_expected));
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_concurrent_invocations() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("concurrent", move |_site, _: ()| {
            let counter = counter_clone.clone();
            async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                counter.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        let mut handles = vec![];
        for _ in 0..10 {
            let engine_clone = engine.clone();
            handles.push(tokio::spawn(async move {
                engine_clone.invoke("concurrent", ()).await.unwrap();
            }));
        }
        
        for handle in handles {
            handle.await.unwrap();
        }
        
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        
        assert_eq!(counter.load(AtomicOrdering::SeqCst), 10);
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_cron_every_second() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("frequent", move |_site, _: ()| {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });

        builder.schedule_cron("frequent", "* * * * * *", ());

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(2500)).await;
        
        let count = counter.load(AtomicOrdering::SeqCst);
        assert!(count >= 2 && count <= 4, "Expected 2-4 executions, got {}", count);
        
        shutdown.notify_one();
    }

    #[tokio::test]
    async fn test_interval_task_reschedules() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut builder = TaskEngineBuilder::new(Tz::UTC);
        builder.register::<(), _, _>("interval", move |_site, _: ()| {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, AtomicOrdering::SeqCst);
            }
        });

        builder.schedule_interval("interval", 500, ());

        let engine = builder.build().unwrap();
        let site = crate::testing::mock_site().await;
        let shutdown = Arc::new(tokio::sync::Notify::new());
        
        engine.start_runner(site.clone(), shutdown.clone()).await.unwrap();
        
        tokio::time::sleep(tokio::time::Duration::from_millis(1600)).await;
        
        let count = counter.load(AtomicOrdering::SeqCst);
        assert!(count >= 2 && count <= 4, "Expected 2-4 executions, got {}", count);
        
        shutdown.notify_one();
    }
}

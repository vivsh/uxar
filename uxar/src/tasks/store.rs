use std::{collections::VecDeque, sync::Arc};

use tokio::sync::{OwnedSemaphorePermit, mpsc};

use crate::{Site, tasks::{TaskDispatcher, TaskError, TaskInput, TaskKind, TaskOutput, TaskRegistry, TaskStatus}};




#[derive(thiserror::Error, Debug)]
pub enum TaskStoreError {
    #[error("Failed to commit task outputs: {source}")]
    CommitOutputFailed {
        #[source]
        source: sqlx::Error,
        outputs: Vec<TaskOutput>,
    },

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
}

pub trait AbstractTaskStore {
    fn claim_tasks(&self) -> impl Future<Output = Result<Vec<TaskInput>, TaskError>> + Send + '_;

    fn commit_outputs(
        &self,
        outputs: Vec<TaskOutput>,
    ) -> impl Future<Output = Result<(), TaskStoreError>> + Send + '_;

    fn store_task(
        &self,
        input: TaskInput,
    ) -> impl Future<Output = Result<(), TaskError>> + Send + '_;

    fn recover_tasks(&self) -> impl Future<Output = Result<(), TaskError>> + Send + '_;

    fn run_migrations(&self) -> impl Future<Output = Result<(), TaskError>> + Send + '_;
}

// Implement TaskStore for Arc<T> to enable shared ownership in concurrent scenarios
impl<T: AbstractTaskStore + ?Sized> AbstractTaskStore for Arc<T> {
    fn claim_tasks(&self) -> impl Future<Output = Result<Vec<TaskInput>, TaskError>> + Send + '_ {
        (**self).claim_tasks()
    }

    fn commit_outputs(
        &self,
        outputs: Vec<TaskOutput>,
    ) -> impl Future<Output = Result<(), TaskStoreError>> + Send + '_ {
        (**self).commit_outputs(outputs)
    }

    fn store_task(
        &self,
        input: TaskInput,
    ) -> impl Future<Output = Result<(), TaskError>> + Send + '_ {
        (**self).store_task(input)
    }

    fn recover_tasks(&self) -> impl Future<Output = Result<(), TaskError>> + Send + '_ {
        (**self).recover_tasks()
    }

    fn run_migrations(&self) -> impl Future<Output = Result<(), TaskError>> + Send + '_ {
        (**self).run_migrations()
    }
}

pub struct AbstractTaskRunner<S: AbstractTaskStore + Send + Sync + 'static> {
    task_queue: VecDeque<Arc<TaskInput>>,
    output_queue: VecDeque<TaskOutput>,
    capacity: usize,
    inflight: usize,
    concurrency: usize,
    poll_interval: tokio::time::Duration,
    notifier: Arc<tokio::sync::Notify>,
    registry: Arc<TaskRegistry>,
    output_sender: mpsc::Sender<TaskOutput>,
    output_receiver: mpsc::Receiver<TaskOutput>,
    store: Arc<S>,
}

impl<T: AbstractTaskStore + Send + Sync + 'static> std::fmt::Debug for AbstractTaskRunner<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskRunner")
            .field("capacity", &self.capacity)
            .field("inflight", &self.inflight)
            .field("task_queue_len", &self.task_queue.len())
            .field("output_queue_len", &self.output_queue.len())
            .finish()
    }
}

impl<S: AbstractTaskStore + Send + Sync + 'static> AbstractTaskRunner<S> {
    pub fn new(dispatcher: TaskDispatcher<S>) -> Self {
        let poll_interval_ms = dispatcher.registry.config.poll_interval_ms;
        let capacity = dispatcher.registry.config.capacity;
        let concurrency = dispatcher.registry.config.concurrency;
        let (output_sender, output_receiver) = mpsc::channel(capacity);
        Self {
            task_queue: VecDeque::new(),
            output_queue: VecDeque::new(),
            inflight: 0,
            capacity,
            concurrency,
            poll_interval: tokio::time::Duration::from_millis(poll_interval_ms as u64),
            notifier: dispatcher.notifier.clone(),
            store: dispatcher.store.clone(),
            registry: dispatcher.registry.clone(),
            output_sender,
            output_receiver,
        }
    }

    fn can_load_tasks(&self) -> bool {
        if self.inflight >= self.capacity {
            return false;
        }

        self.task_queue.len() < self.capacity / 2
    }

    fn can_flush_outputs(&self) -> bool {
        self.output_queue.len() >= self.capacity / 2
    }

    fn insert_task(&mut self, task: Arc<TaskInput>) {
        self.inflight += 1;
        self.task_queue.push_back(task);
    }

    fn insert_output(&mut self, output: TaskOutput) {
        self.inflight = self.inflight.saturating_sub(1);
        self.output_queue.push_back(output);
    }

    async fn load_tasks(&mut self) -> Option<usize> {
        if !self.can_load_tasks() {
            return None;
        }

        match self.store.claim_tasks().await {
            Ok(tasks) => {
                let count = tasks.len();
                for task in tasks {
                    self.insert_task(Arc::new(task));
                }
                return Some(count);
            }
            Err(e) => {
                tracing::error!("Failed to load tasks: {}", e);
                return Some(0);
            }
        }
    }

    async fn flush_outputs(&mut self, force: bool) {
        if !self.can_flush_outputs() && !force {
            return;
        }
        let outputs = self.output_queue.drain(..).collect::<Vec<_>>();
        match self.store.commit_outputs(outputs).await {
            Ok(_) => {}
            Err(TaskStoreError::CommitOutputFailed { source, outputs }) => {
                tracing::error!("Failed to flush task outputs: {}", source);
                // re-insert failed outputs back into the queue
                for output in outputs {
                    self.output_queue.push_back(output);
                    self.inflight += 1;
                }
            }
            Err(e) => {
                // self.output_queue.extend(outputs);
                // TODO: handle failed outputs more gracefully
                tracing::error!("Failed to flush task outputs: {}", e);
            }
        }
    }

    pub fn run_concurrently(
        &self,
        site: Site,
        permit: OwnedSemaphorePermit,
        input: Arc<TaskInput>,
    ) -> tokio::task::JoinHandle<()> {
        let engine = self.registry.clone();
        let sender = self.output_sender.clone();
        tokio::spawn(async move {
            let _ = permit;
            let output = engine.execute(site, input.clone()).await;
            if let Err(e) = sender.send(output).await {
                tracing::error!("Failed to send task output for task {}: {}", input.id, e);
            }
        })
    }

    fn run_flows(&mut self, site: Site) {
        let total = self.task_queue.len();
        for _ in 0..total {
            if let Some(task) = self.task_queue.pop_front() {
                if let Some(service) = self.registry.tasks.get(task.name()) {
                    if matches!(service.kind, TaskKind::Flow) {
                        let output = self.registry.execute_flow(site.clone(), task.clone());
                        self.insert_output(output);
                    } else {
                        self.task_queue.push_back(task);
                    }
                } else {
                    let output = TaskOutput::from_error(
                        task.id,
                        TaskError::TaskNotFound(task.name().to_string()),
                    );
                    self.insert_output(output);
                }
            }
        }
    }

    pub async fn run(mut self, site: Site) {
        let shutdown = site.shutdown_notifier();
        let mut backoff_delay = self.poll_interval;
        let max_backoff = self.poll_interval * 32;
        let mut force_flush = false;
        let sem = Arc::new(tokio::sync::Semaphore::new(self.concurrency));

        loop {
            // outputs should be immediately flushed if there are no tasks to load
            self.flush_outputs(force_flush).await;
            let loaded = self.load_tasks().await;
            if let Some(count) = loaded {
                if count == 0 {
                    if !self.output_queue.is_empty() {
                        force_flush = true;
                    } else {
                        force_flush = false;
                        backoff_delay = (backoff_delay * 2).min(max_backoff);
                        tracing::debug!("No tasks loaded, backing off to {:?}", backoff_delay);
                    }
                } else {
                    force_flush = false;
                    backoff_delay = self.poll_interval;
                    tracing::info!("Loaded {} tasks", count);
                }
            }

            // flow type tasks are run synchronously to preserve order
            // they dont need concurrency or rate limiting
            self.run_flows(site.clone());

            tokio::select! {
                _ = shutdown.notified() => {
                    tracing::info!("TaskRunner shutting down");
                    break;
                },
                permit_result = sem.clone().acquire_owned(), if self.task_queue.len() > 0 => {
                    match permit_result {
                        Ok(permit) => {
                            if let Some(task_input) = self.task_queue.pop_front(){
                                self.run_concurrently(site.clone(), permit, task_input);
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to acquire semaphore permit: {}", e);
                            break;
                        }
                    }
                },
                Some(output) = self.output_receiver.recv() => {
                    self.insert_output(output);
                },
                _ = self.notifier.notified() => {
                    backoff_delay = self.poll_interval;
                },
                _ = tokio::time::sleep(backoff_delay) => {},
            }
        }
    }
}

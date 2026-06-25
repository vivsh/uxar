use std::sync::Arc;

use crate::tasks::{
    TaskError, TaskListFilter, TaskListPage, TaskOutcome, TaskRecord, TaskStatus,
    store::AbstractTaskStore,
};

/// In-memory task store for tests and local development.
///
/// This store is not durable and must not be used for reliable background work.
#[derive(Clone)]
pub struct MemoryTaskStore {
    tasks: Arc<tokio::sync::RwLock<Vec<TaskRecord>>>,
    batch_size: usize,
    lease_duration: chrono::Duration,
}

impl MemoryTaskStore {
    pub fn new(batch_size: usize) -> Self {
        Self::with_lease_duration(batch_size, std::time::Duration::from_secs(300))
    }

    pub fn with_lease_duration(batch_size: usize, lease_duration: std::time::Duration) -> Self {
        Self {
            tasks: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            batch_size: batch_size.max(1),
            lease_duration: chrono::Duration::from_std(lease_duration).unwrap_or_default(),
        }
    }

    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    pub async fn tasks(&self) -> Vec<TaskRecord> {
        self.tasks.read().await.clone()
    }
}

fn is_active_status(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Pending | TaskStatus::Running | TaskStatus::Suspended
    )
}

impl AbstractTaskStore for MemoryTaskStore {
    async fn claim_tasks(&self, runner_id: &str) -> Result<Vec<TaskRecord>, TaskError> {
        let mut tasks = self.tasks.write().await;
        let now = chrono::Utc::now();

        let mut ready_indices: Vec<_> = tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                (task.status == TaskStatus::Pending
                    && task.ready_at.map(|ts| ts <= now).unwrap_or(true))
                    || (task.status == TaskStatus::Running
                        && task.leased_until.is_some_and(|ts| ts <= now))
            })
            .map(|(i, task)| {
                let ready_at = if task.status == TaskStatus::Running {
                    task.leased_until.unwrap_or(now)
                } else {
                    task.ready_at.unwrap_or(now)
                };
                (
                    i,
                    std::cmp::Reverse(task.priority),
                    ready_at,
                    task.created_at,
                )
            })
            .collect();

        ready_indices
            .sort_by_key(|(_, priority, ready_at, created_at)| (*priority, *ready_at, *created_at));

        let claimed = ready_indices
            .iter()
            .take(self.batch_size)
            .map(|(i, _, _, _)| {
                let task = &mut tasks[*i];
                task.status = TaskStatus::Running;
                task.locked_by = Some(runner_id.to_string());
                let lease_duration = task
                    .lease_duration_ms
                    .map(chrono::Duration::milliseconds)
                    .unwrap_or(self.lease_duration);
                task.leased_until = Some(now + lease_duration);
                task.updated_at = now;
                task.clone()
            })
            .collect();

        Ok(claimed)
    }

    async fn commit_outcome(
        &self,
        task_id: uuid::Uuid,
        runner_id: &str,
        outcome: TaskOutcome,
    ) -> Result<(), TaskError> {
        let mut tasks = self.tasks.write().await;
        let Some(task) = tasks.iter_mut().find(|task| {
            task.id == task_id
                && task.status == TaskStatus::Running
                && task.locked_by.as_deref() == Some(runner_id)
        }) else {
            return Ok(());
        };

        let now = chrono::Utc::now();
        match outcome {
            TaskOutcome::Complete { result } => {
                task.status = TaskStatus::Succeeded;
                task.result = Some(result);
                task.completed_at = Some(now);
                task.ready_at = None;
            }
            TaskOutcome::Suspend { state, output } => {
                task.status = TaskStatus::Suspended;
                task.state = Some(state);
                task.output = output;
                task.ready_at = None;
            }
            TaskOutcome::Sleep { state, delay } => {
                task.status = TaskStatus::Pending;
                task.state = Some(state);
                task.ready_at = Some(now + chrono::Duration::from_std(delay).unwrap_or_default());
            }
            TaskOutcome::Retry { delay, error } => {
                task.attempts += 1;
                task.last_error = Some(error);
                if task.max_attempts.is_some_and(|max| task.attempts >= max) {
                    task.status = TaskStatus::Failed;
                    task.ready_at = None;
                    task.completed_at = Some(now);
                } else {
                    task.status = TaskStatus::Pending;
                    let retry_delay = delay
                        .map(chrono::Duration::from_std)
                        .transpose()
                        .unwrap_or_default()
                        .unwrap_or_else(|| {
                            task.retry_delay_ms
                                .map(chrono::Duration::milliseconds)
                                .unwrap_or_default()
                        });
                    task.ready_at = Some(now + retry_delay);
                }
            }
            TaskOutcome::Fail { error } => {
                task.status = TaskStatus::Failed;
                task.last_error = Some(error);
                task.ready_at = None;
                task.completed_at = Some(now);
            }
        }

        task.locked_by = None;
        task.leased_until = None;
        task.updated_at = now;
        Ok(())
    }

    async fn store_task(&self, record: TaskRecord) -> Result<(), TaskError> {
        let mut tasks = self.tasks.write().await;
        if tasks.iter().any(|task| task.id == record.id) {
            return Err(TaskError::AlreadyExists(record.id.to_string()));
        }
        if let Some(identity) = record.identity.as_deref()
            && is_active_status(record.status)
            && tasks.iter().any(|task| {
                task.identity.as_deref() == Some(identity) && is_active_status(task.status)
            })
        {
            return Err(TaskError::IdentityError);
        }
        tasks.push(record);
        Ok(())
    }

    async fn resume(&self, id: uuid::Uuid, input: String) -> Result<u64, TaskError> {
        let mut tasks = self.tasks.write().await;
        let now = chrono::Utc::now();
        let mut count = 0;
        for task in tasks.iter_mut() {
            if task.id == id && task.status == TaskStatus::Suspended {
                task.status = TaskStatus::Pending;
                task.resume_input = Some(input.clone());
                task.ready_at = Some(now);
                task.updated_at = now;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn list_tasks(&self, filter: TaskListFilter) -> Result<TaskListPage, TaskError> {
        let mut records = self
            .tasks
            .read()
            .await
            .iter()
            .filter(|task| filter.status.is_none_or(|status| task.status == status))
            .filter(|task| filter.name.as_deref().is_none_or(|name| task.name == name))
            .filter(|task| {
                filter
                    .identity
                    .as_deref()
                    .is_none_or(|identity| task.identity.as_deref() == Some(identity))
            })
            .filter(|task| {
                filter
                    .priority_min
                    .is_none_or(|priority_min| task.priority >= priority_min)
            })
            .filter(|task| {
                filter.q.as_deref().is_none_or(|q| {
                    let q = q.to_lowercase();
                    task.name.to_lowercase().contains(&q)
                        || task
                            .identity
                            .as_ref()
                            .is_some_and(|value| value.to_lowercase().contains(&q))
                        || task
                            .last_error
                            .as_ref()
                            .is_some_and(|value| value.to_lowercase().contains(&q))
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|task| std::cmp::Reverse(task.created_at));
        let page = records
            .into_iter()
            .skip(filter.offset)
            .take(filter.limit + 1)
            .collect::<Vec<_>>();
        if page.len() > filter.limit {
            Ok(TaskListPage {
                records: page.into_iter().take(filter.limit).collect(),
                next_cursor: Some((filter.offset + filter.limit).to_string()),
            })
        } else {
            Ok(TaskListPage {
                records: page,
                next_cursor: None,
            })
        }
    }

    async fn get_task(&self, id: uuid::Uuid) -> Result<Option<TaskRecord>, TaskError> {
        Ok(self
            .tasks
            .read()
            .await
            .iter()
            .find(|task| task.id == id)
            .cloned())
    }

    async fn run_migrations(&self) -> Result<(), TaskError> {
        Ok(())
    }
}

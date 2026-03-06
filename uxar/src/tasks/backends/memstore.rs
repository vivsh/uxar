use std::sync::Arc;

use crate::tasks::{TaskError, TaskInput, TaskOutput, TaskStatus, store::{AbstractTaskStore, TaskStoreError}};




/// In-memory task store for testing
pub struct MemoryTaskStore {
    tasks: Arc<tokio::sync::RwLock<Vec<TaskInput>>>,
    outputs: Arc<tokio::sync::RwLock<Vec<TaskOutput>>>,
    batch_size: usize,
}

impl MemoryTaskStore {
    pub fn new(batch_size: usize) -> Self {
        Self {
            tasks: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            outputs: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            batch_size,
        }
    }

    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    pub async fn output_count(&self) -> usize {
        self.outputs.read().await.len()
    }
}

impl AbstractTaskStore for MemoryTaskStore {
    async fn claim_tasks(&self) -> Result<Vec<TaskInput>, TaskError> {
        let mut tasks = self.tasks.write().await;
        let now = chrono::Utc::now();

        // Collect indices of ready tasks
        let mut ready_indices: Vec<_> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status == TaskStatus::Ready && t.ready_time <= now)
            .map(|(i, t)| (i, t.ready_time))
            .collect();

        // Sort by ready_time (oldest first)
        ready_indices.sort_by_key(|(_, ready_time)| *ready_time);

        // Claim up to batch_size tasks
        let claimed: Vec<TaskInput> = ready_indices
            .iter()
            .take(self.batch_size)
            .map(|(i, _)| {
                let task = &mut tasks[*i];
                task.status = TaskStatus::Enqueued;
                task.clone()
            })
            .collect();

        Ok(claimed)
    }

    async fn commit_outputs(&self, outputs: Vec<TaskOutput>) -> Result<(), TaskStoreError> {
        let mut task_store = self.tasks.write().await;
        let mut output_store = self.outputs.write().await;

        for output in outputs {
            output_store.push(output.clone());

            if let Some(task) = task_store.iter_mut().find(|t| t.id == output.task_id) {
                task.status = output.status.clone();
            }
        }

        Ok(())
    }

    async fn store_task(&self, input: TaskInput) -> Result<(), TaskError> {
        let mut tasks = self.tasks.write().await;
        tasks.push(input);
        Ok(())
    }

    async fn recover_tasks(&self) -> Result<(), TaskError> {
        let mut tasks = self.tasks.write().await;
        for task in tasks.iter_mut() {
            if task.status == TaskStatus::Enqueued {
                task.status = TaskStatus::Ready;
            }
        }
        Ok(())
    }

    async fn run_migrations(&self) -> Result<(), TaskError> {
        Ok(())
    }
}

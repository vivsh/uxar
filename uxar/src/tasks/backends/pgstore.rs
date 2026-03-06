use crate::{db::Pool, tasks::{TaskError, store::{AbstractTaskStore, TaskStoreError}}};
use super::super::{TaskInput, TaskOutput, TaskStatus};



#[derive(Clone)]
pub struct PgTaskStore {
    pool: Pool,
    batch_size: usize,
}

impl PgTaskStore {
    pub fn new(pool: Pool, batch_size: usize) -> Self {
        Self { pool, batch_size }
    }
}

impl AbstractTaskStore for PgTaskStore {
    async fn claim_tasks(&self) -> Result<Vec<TaskInput>, TaskError> {
        let tasks: Vec<TaskInput> = sqlx::query_as(
            r#"
            UPDATE uxar.tasks
            SET status = $1
            WHERE id IN (
                SELECT id FROM uxar.tasks
                WHERE status = $2
                AND ready_time <= NOW()
                ORDER BY ready_time
                LIMIT $3
                FOR UPDATE SKIP LOCKED
            )
            RETURNING *
            "#,
        )
        .bind(TaskStatus::Enqueued as i16)
        .bind(TaskStatus::Ready as i16)
        .bind(self.batch_size as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;

        Ok(tasks)
    }

    async fn commit_outputs(&self, outputs: Vec<TaskOutput>) -> Result<(), TaskStoreError> {
        if outputs.is_empty() {
            return Ok(());
        }

        let size = outputs.len();
        let mut output_ids = Vec::with_capacity(size);
        let mut task_ids = Vec::with_capacity(size);
        let mut child_ids = Vec::with_capacity(size);
        let mut statuses = Vec::with_capacity(size);
        let mut data_values = Vec::with_capacity(size);
        let mut create_times = Vec::with_capacity(size);
        let mut states = Vec::with_capacity(size);

        for output in &outputs {
            let status_val = output.status.clone() as i16;
            output_ids.push(output.id);
            task_ids.push(output.task_id);
            child_ids.push(output.child_id);
            statuses.push(status_val);

            // Extract state from data for Suspend status
            let state = if output.status == TaskStatus::Suspend && !output.data.is_empty() {
                Some(output.data.clone())
            } else {
                None
            };
            states.push(state);

            data_values.push(output.data.as_str());
            create_times.push(output.create_time);
        }

        sqlx::query(
            r#"
            WITH inserted AS (
                INSERT INTO uxar.task_outputs (id, task_id, child_id, status, data, create_time)
                SELECT * FROM UNNEST($1::uuid[], $2::uuid[], $3::uuid[], $4::smallint[], $5::text[], $6::timestamptz[])
                RETURNING task_id, status
            )
            UPDATE uxar.tasks t
            SET status = u.status, state = u.state
            FROM (
                SELECT 
                    UNNEST($2::uuid[]) as task_id,
                    UNNEST($4::smallint[]) as status,
                    UNNEST($7::text[]) as state
            ) u
            WHERE t.id = u.task_id
            "#,
        )
        .bind(&output_ids)
        .bind(&task_ids)
        .bind(&child_ids)
        .bind(&statuses)
        .bind(&data_values)
        .bind(&create_times)
        .bind(&states)
        .execute(&self.pool)
        .await
        .map_err(|e| TaskStoreError::CommitOutputFailed {
            source: e,
            outputs,
        })?;

        Ok(())
    }

    async fn store_task(&self, input: TaskInput) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            INSERT INTO uxar.tasks (id, root_id, name, child_id, data, state, ready_time, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(input.id)
        .bind(input.root_id)
        .bind(&input.name)
        .bind(input.child_id)
        .bind(&input.data)
        .bind(&input.state)
        .bind(input.ready_time)
        .bind(input.status.clone() as i16)
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;
        Ok(())
    }

    async fn recover_tasks(&self) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            UPDATE uxar.tasks
            SET status = $1
            WHERE status = $2
            "#,
        )
        .bind(TaskStatus::Ready as i16)
        .bind(TaskStatus::Enqueued as i16)
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;
        Ok(())
    }

    async fn run_migrations(&self) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            CREATE SCHEMA IF NOT EXISTS uxar
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS uxar.tasks (
                id UUID PRIMARY KEY,
                root_id UUID NOT NULL,
                name TEXT NOT NULL,
                child_id UUID,
                data TEXT NOT NULL,
                state TEXT,
                ready_time TIMESTAMPTZ NOT NULL,
                status SMALLINT NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_status_ready
            ON uxar.tasks(status, ready_time)
            WHERE status = 0
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS uxar.task_outputs (
                id UUID PRIMARY KEY,
                task_id UUID NOT NULL REFERENCES uxar.tasks(id),
                child_id UUID,
                status SMALLINT NOT NULL,
                data TEXT NOT NULL,
                create_time TIMESTAMPTZ NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| TaskError::Other(Box::new(e)))?;

        Ok(())
    }
}

use crate::{
    db::Pool,
    tasks::{TaskError, TaskOutcome, TaskRecord, TaskStatus, store::AbstractTaskStore},
};

#[derive(Clone)]
pub struct PgTaskStore {
    pool: Pool,
    batch_size: usize,
    lease_duration: chrono::Duration,
}

impl PgTaskStore {
    pub fn new(pool: Pool, batch_size: usize, lease_duration: std::time::Duration) -> Self {
        Self {
            pool,
            batch_size: batch_size.max(1),
            lease_duration: chrono::Duration::from_std(lease_duration).unwrap_or_default(),
        }
    }

    fn map_store_error(err: sqlx::Error) -> TaskError {
        if let sqlx::Error::Database(db_err) = &err
            && db_err.code().as_deref() == Some("23505")
        {
            let message = db_err.message();
            if message.contains("idx_tasks_active_identity") {
                return TaskError::IdentityError;
            }
        }
        TaskError::DatabaseError(err)
    }

    async fn update_running(
        &self,
        task_id: uuid::Uuid,
        runner_id: &str,
        status: TaskStatus,
        state: Option<String>,
        resume_topic: Option<String>,
        resume_input: Option<String>,
        output: Option<String>,
        result: Option<String>,
        last_error: Option<String>,
        ready_at: Option<chrono::DateTime<chrono::Utc>>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
        increment_attempts: bool,
    ) -> Result<(), TaskError> {
        let rows = sqlx::query(
            r#"
            UPDATE vyuh.tasks
            SET status = $1,
                state = COALESCE($2, state),
                resume_topic = $3,
                resume_input = $4,
                output = $5,
                result = $6,
                last_error = $7,
                ready_at = $8,
                completed_at = $9,
                attempts = attempts + CASE WHEN $10 THEN 1 ELSE 0 END,
                locked_by = NULL,
                leased_until = NULL,
                updated_at = NOW()
            WHERE id = $11
              AND locked_by = $12
              AND status = $13
            "#,
        )
        .bind(status)
        .bind(state)
        .bind(resume_topic)
        .bind(resume_input)
        .bind(output)
        .bind(result)
        .bind(last_error)
        .bind(ready_at)
        .bind(completed_at)
        .bind(increment_attempts)
        .bind(task_id)
        .bind(runner_id)
        .bind(TaskStatus::Running)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if rows == 0 {
            tracing::warn!(
                "Task {} outcome ignored because runner {} no longer owns it",
                task_id,
                runner_id
            );
        }

        Ok(())
    }
}

impl AbstractTaskStore for PgTaskStore {
    async fn claim_tasks(&self, runner_id: &str) -> Result<Vec<TaskRecord>, TaskError> {
        let default_lease_ms = self.lease_duration.num_milliseconds();
        let tasks = sqlx::query_as::<_, TaskRecord>(
            r#"
            UPDATE vyuh.tasks
            SET status = $1,
                locked_by = $2,
                leased_until = NOW() + (COALESCE(lease_duration_ms, $3) * INTERVAL '1 millisecond'),
                updated_at = NOW()
            WHERE id IN (
                SELECT id FROM vyuh.tasks
                WHERE (
                    status = $4
                    AND (ready_at IS NULL OR ready_at <= NOW())
                ) OR (
                    status = $1
                    AND leased_until IS NOT NULL
                    AND leased_until <= NOW()
                )
                ORDER BY
                    CASE
                        WHEN status = $1 THEN leased_until
                        ELSE COALESCE(ready_at, NOW())
                    END,
                    created_at
                LIMIT $5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING
                id, name, input, state, resume_topic, resume_input, output, result,
                status, attempts, max_attempts, retry_delay_ms, lease_duration_ms,
                last_error, identity, locked_by, leased_until, ready_at, created_at,
                updated_at, completed_at
            "#,
        )
        .bind(TaskStatus::Running)
        .bind(runner_id)
        .bind(default_lease_ms)
        .bind(TaskStatus::Pending)
        .bind(self.batch_size as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(tasks)
    }

    async fn commit_outcome(
        &self,
        task_id: uuid::Uuid,
        runner_id: &str,
        outcome: TaskOutcome,
    ) -> Result<(), TaskError> {
        let now = chrono::Utc::now();
        match outcome {
            TaskOutcome::Complete { result } => {
                self.update_running(
                    task_id,
                    runner_id,
                    TaskStatus::Succeeded,
                    None,
                    None,
                    None,
                    None,
                    Some(result),
                    None,
                    None,
                    Some(now),
                    false,
                )
                .await
            }
            TaskOutcome::Suspend {
                topic,
                state,
                output,
            } => {
                self.update_running(
                    task_id,
                    runner_id,
                    TaskStatus::Suspended,
                    Some(state),
                    Some(topic),
                    None,
                    output,
                    None,
                    None,
                    None,
                    None,
                    false,
                )
                .await
            }
            TaskOutcome::Sleep { state, delay } => {
                self.update_running(
                    task_id,
                    runner_id,
                    TaskStatus::Pending,
                    Some(state),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(now + chrono::Duration::from_std(delay).unwrap_or_default()),
                    None,
                    false,
                )
                .await
            }
            TaskOutcome::Retry { delay, error } => {
                let next_ready =
                    delay.map(|delay| now + chrono::Duration::from_std(delay).unwrap_or_default());
                let rows = sqlx::query(
                    r#"
                    UPDATE vyuh.tasks
                    SET status = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN $1 ELSE $2 END,
                        attempts = attempts + 1,
                        last_error = $3,
                        ready_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN NULL
                            ELSE COALESCE(
                                $4,
                                CASE
                                    WHEN retry_delay_ms IS NULL THEN NOW()
                                    ELSE NOW() + (retry_delay_ms * INTERVAL '1 millisecond')
                                END
                            )
                        END,
                        completed_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN NOW() ELSE NULL END,
                        locked_by = NULL,
                        leased_until = NULL,
                        updated_at = NOW()
                    WHERE id = $5
                      AND locked_by = $6
                      AND status = $7
                    "#,
                )
                .bind(TaskStatus::Failed)
                .bind(TaskStatus::Pending)
                .bind(error)
                .bind(next_ready)
                .bind(task_id)
                .bind(runner_id)
                .bind(TaskStatus::Running)
                .execute(&self.pool)
                .await?
                .rows_affected();
                if rows == 0 {
                    tracing::warn!(
                        "Task {} retry outcome ignored because runner {} no longer owns it",
                        task_id,
                        runner_id
                    );
                }
                Ok(())
            }
            TaskOutcome::Fail { error } => {
                self.update_running(
                    task_id,
                    runner_id,
                    TaskStatus::Failed,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(error),
                    None,
                    Some(now),
                    false,
                )
                .await
            }
        }
    }

    async fn store_task(&self, record: TaskRecord) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            INSERT INTO vyuh.tasks (
                id, name, input, state, resume_topic, resume_input, output, result,
                status, attempts, max_attempts, retry_delay_ms, lease_duration_ms, last_error, identity,
                locked_by, leased_until, ready_at, created_at, updated_at, completed_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21
            )
            "#,
        )
        .bind(record.id)
        .bind(&record.name)
        .bind(&record.input)
        .bind(&record.state)
        .bind(&record.resume_topic)
        .bind(&record.resume_input)
        .bind(&record.output)
        .bind(&record.result)
        .bind(record.status)
        .bind(record.attempts)
        .bind(record.max_attempts)
        .bind(record.retry_delay_ms)
        .bind(record.lease_duration_ms)
        .bind(&record.last_error)
        .bind(&record.identity)
        .bind(&record.locked_by)
        .bind(record.leased_until)
        .bind(record.ready_at)
        .bind(record.created_at)
        .bind(record.updated_at)
        .bind(record.completed_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_store_error)?;
        Ok(())
    }

    async fn resume(&self, topic: &str, input: String) -> Result<u64, TaskError> {
        let rows = sqlx::query(
            r#"
            UPDATE vyuh.tasks
            SET status = $1,
                resume_input = $2,
                resume_topic = NULL,
                ready_at = NOW(),
                updated_at = NOW()
            WHERE status = $3
              AND resume_topic = $4
            "#,
        )
        .bind(TaskStatus::Pending)
        .bind(input)
        .bind(TaskStatus::Suspended)
        .bind(topic)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows)
    }

    async fn run_migrations(&self) -> Result<(), TaskError> {
        sqlx::query("CREATE SCHEMA IF NOT EXISTS vyuh")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS vyuh.tasks (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                input TEXT NOT NULL,
                state TEXT,
                resume_topic TEXT,
                resume_input TEXT,
                output TEXT,
                result TEXT,
                status SMALLINT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER,
                retry_delay_ms BIGINT,
                lease_duration_ms BIGINT,
                last_error TEXT,
                identity TEXT,
                locked_by TEXT,
                leased_until TIMESTAMPTZ,
                ready_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                completed_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_pending_claim
            ON vyuh.tasks(status, ready_at, created_at)
            WHERE status = 0
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_suspended_topic
            ON vyuh.tasks(resume_topic)
            WHERE status = 2
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_running_lease
            ON vyuh.tasks(status, leased_until)
            WHERE status = 1
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_active_identity
            ON vyuh.tasks(identity)
            WHERE identity IS NOT NULL
              AND status IN (0, 1, 2)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_name_status
            ON vyuh.tasks(name, status)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

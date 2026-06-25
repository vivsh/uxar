use crate::{
    db::Pool,
    tasks::{
        TaskError, TaskListFilter, TaskListPage, TaskOutcome, TaskRecord, TaskStatus,
        store::AbstractTaskStore,
    },
};
use sqlx::{Postgres, QueryBuilder};

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
                resume_input = $3,
                output = $4,
                result = $5,
                last_error = $6,
                ready_at = $7,
                completed_at = $8,
                attempts = attempts + CASE WHEN $9 THEN 1 ELSE 0 END,
                locked_by = NULL,
                leased_until = NULL,
                updated_at = NOW()
            WHERE id = $10
              AND locked_by = $11
              AND status = $12
            "#,
        )
        .bind(status)
        .bind(state)
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
        let mut tasks = sqlx::query_as::<_, TaskRecord>(
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
                    priority DESC,
                    CASE
                        WHEN status = $1 THEN leased_until
                        ELSE COALESCE(ready_at, NOW())
                    END,
                    created_at
                LIMIT $5
                FOR UPDATE SKIP LOCKED
            )
            RETURNING
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
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

        crate::tasks::sort_claimed_tasks(&mut tasks);
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
                    Some(result),
                    None,
                    None,
                    Some(now),
                    false,
                )
                .await
            }
            TaskOutcome::Suspend { state, output } => {
                self.update_running(
                    task_id,
                    runner_id,
                    TaskStatus::Suspended,
                    Some(state),
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
                    Some(error),
                    None,
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
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms, last_error, identity,
                locked_by, leased_until, ready_at, created_at, updated_at, completed_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11, $12, $13, $14, $15,
                $16, $17, $18, $19, $20, $21
            )
            "#,
        )
        .bind(record.id)
        .bind(&record.name)
        .bind(&record.input)
        .bind(&record.state)
        .bind(&record.resume_input)
        .bind(&record.output)
        .bind(&record.result)
        .bind(record.status)
        .bind(record.attempts)
        .bind(record.priority)
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

    async fn resume(&self, id: uuid::Uuid, input: String) -> Result<u64, TaskError> {
        let rows = sqlx::query(
            r#"
            UPDATE vyuh.tasks
            SET status = $1,
                resume_input = $2,
                ready_at = NOW(),
                updated_at = NOW()
            WHERE id = $3
              AND status = $4
            "#,
        )
        .bind(TaskStatus::Pending)
        .bind(input)
        .bind(id)
        .bind(TaskStatus::Suspended)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows)
    }

    async fn list_tasks(&self, filter: TaskListFilter) -> Result<TaskListPage, TaskError> {
        let mut builder = QueryBuilder::<Postgres>::new(
            r#"
            SELECT
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
                last_error, identity, locked_by, leased_until, ready_at, created_at,
                updated_at, completed_at
            FROM vyuh.tasks
            WHERE 1 = 1
            "#,
        );
        push_filters(&mut builder, &filter);
        builder.push(" ORDER BY created_at DESC, id DESC LIMIT ");
        builder.push_bind((filter.limit + 1) as i64);
        builder.push(" OFFSET ");
        builder.push_bind(filter.offset as i64);

        let mut records = builder
            .build_query_as::<TaskRecord>()
            .fetch_all(&self.pool)
            .await?;
        let next_cursor = if records.len() > filter.limit {
            records.truncate(filter.limit);
            Some((filter.offset + filter.limit).to_string())
        } else {
            None
        };
        Ok(TaskListPage {
            records,
            next_cursor,
        })
    }

    async fn get_task(&self, id: uuid::Uuid) -> Result<Option<TaskRecord>, TaskError> {
        sqlx::query_as::<_, TaskRecord>(
            r#"
            SELECT
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
                last_error, identity, locked_by, leased_until, ready_at, created_at,
                updated_at, completed_at
            FROM vyuh.tasks
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(TaskError::from)
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
                resume_input TEXT,
                output TEXT,
                result TEXT,
                status SMALLINT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0,
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
            ALTER TABLE vyuh.tasks
            ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_tasks_pending_priority_claim
            ON vyuh.tasks(status, priority DESC, ready_at, created_at)
            WHERE status = 0
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

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, filter: &'a TaskListFilter) {
    if let Some(status) = filter.status {
        builder.push(" AND status = ");
        builder.push_bind(status);
    }
    if let Some(name) = &filter.name {
        builder.push(" AND name = ");
        builder.push_bind(name);
    }
    if let Some(identity) = &filter.identity {
        builder.push(" AND identity = ");
        builder.push_bind(identity);
    }
    if let Some(priority_min) = filter.priority_min {
        builder.push(" AND priority >= ");
        builder.push_bind(priority_min);
    }
    if let Some(q) = &filter.q {
        let q = format!("%{}%", q);
        builder.push(" AND (name ILIKE ");
        builder.push_bind(q.clone());
        builder.push(" OR identity ILIKE ");
        builder.push_bind(q.clone());
        builder.push(" OR last_error ILIKE ");
        builder.push_bind(q);
        builder.push(")");
    }
}

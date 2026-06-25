use crate::{
    db::Pool,
    tasks::{
        TaskError, TaskListFilter, TaskListPage, TaskOutcome, TaskRecord, TaskStatus,
        store::AbstractTaskStore,
    },
};
use sqlx::{QueryBuilder, Sqlite};

#[derive(Clone)]
pub struct SqliteTaskStore {
    pool: Pool,
    batch_size: usize,
    lease_duration: chrono::Duration,
}

impl SqliteTaskStore {
    pub fn new(pool: Pool, batch_size: usize, lease_duration: std::time::Duration) -> Self {
        Self {
            pool,
            batch_size: batch_size.max(1),
            lease_duration: chrono::Duration::from_std(lease_duration).unwrap_or_default(),
        }
    }

    fn map_store_error(err: sqlx::Error) -> TaskError {
        if let sqlx::Error::Database(db_err) = &err {
            let message = db_err.message();
            if message.contains("idx_vyuh_tasks_active_identity")
                || message.contains("UNIQUE constraint failed: vyuh_tasks.identity")
            {
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
            UPDATE vyuh_tasks
            SET status = ?1,
                state = COALESCE(?2, state),
                resume_input = ?3,
                output = ?4,
                result = ?5,
                last_error = ?6,
                ready_at = ?7,
                completed_at = ?8,
                attempts = attempts + CASE WHEN ?9 THEN 1 ELSE 0 END,
                locked_by = NULL,
                leased_until = NULL,
                updated_at = ?10
            WHERE id = ?11
              AND locked_by = ?12
              AND status = ?13
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
        .bind(chrono::Utc::now())
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

impl AbstractTaskStore for SqliteTaskStore {
    async fn claim_tasks(&self, runner_id: &str) -> Result<Vec<TaskRecord>, TaskError> {
        let now = chrono::Utc::now();
        let default_lease_ms = self.lease_duration.num_milliseconds();
        let mut tasks = sqlx::query_as::<_, TaskRecord>(
            r#"
            UPDATE vyuh_tasks
            SET status = ?1,
                locked_by = ?2,
                leased_until = strftime(
                    '%Y-%m-%dT%H:%M:%fZ',
                    ?3,
                    printf('+%d milliseconds', COALESCE(lease_duration_ms, ?4))
                ),
                updated_at = ?3
            WHERE id IN (
                SELECT id
                FROM vyuh_tasks
                WHERE (
                    status = ?5
                    AND (ready_at IS NULL OR ready_at <= ?3)
                ) OR (
                    status = ?1
                    AND leased_until IS NOT NULL
                    AND leased_until <= ?3
                )
                ORDER BY
                    priority DESC,
                    CASE
                        WHEN status = ?1 THEN leased_until
                        ELSE COALESCE(ready_at, ?3)
                    END,
                    created_at
                LIMIT ?6
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
        .bind(now)
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
                let retry_delay_ms: Option<i64> = sqlx::query_scalar(
                    r#"
                    SELECT retry_delay_ms
                    FROM vyuh_tasks
                    WHERE id = ?1
                      AND locked_by = ?2
                      AND status = ?3
                    "#,
                )
                .bind(task_id)
                .bind(runner_id)
                .bind(TaskStatus::Running)
                .fetch_optional(&self.pool)
                .await?
                .flatten();
                let retry_delay = delay
                    .map(chrono::Duration::from_std)
                    .transpose()
                    .unwrap_or_default()
                    .unwrap_or_else(|| {
                        retry_delay_ms
                            .map(chrono::Duration::milliseconds)
                            .unwrap_or_default()
                    });
                let next_ready = now + retry_delay;
                let rows = sqlx::query(
                    r#"
                    UPDATE vyuh_tasks
                    SET status = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN ?1 ELSE ?2 END,
                        attempts = attempts + 1,
                        last_error = ?3,
                        ready_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN NULL ELSE ?4 END,
                        completed_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN ?5 ELSE NULL END,
                        locked_by = NULL,
                        leased_until = NULL,
                        updated_at = ?5
                    WHERE id = ?6
                      AND locked_by = ?7
                      AND status = ?8
                    "#,
                )
                .bind(TaskStatus::Failed)
                .bind(TaskStatus::Pending)
                .bind(error)
                .bind(next_ready)
                .bind(now)
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
                    None,        // state
                    None,        // resume_input
                    None,        // output
                    None,        // result
                    Some(error), // last_error
                    None,        // ready_at
                    Some(now),   // completed_at
                    false,
                )
                .await
            }
        }
    }

    async fn store_task(&self, record: TaskRecord) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            INSERT INTO vyuh_tasks (
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms, last_error, identity,
                locked_by, leased_until, ready_at, created_at, updated_at, completed_at
            )
            VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                ?16, ?17, ?18, ?19, ?20, ?21
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
        let now = chrono::Utc::now();
        let rows = sqlx::query(
            r#"
            UPDATE vyuh_tasks
            SET status = ?1,
                resume_input = ?2,
                ready_at = ?3,
                updated_at = ?3
            WHERE id = ?4
              AND status = ?5
            "#,
        )
        .bind(TaskStatus::Pending)
        .bind(input)
        .bind(now)
        .bind(id)
        .bind(TaskStatus::Suspended)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows)
    }

    async fn list_tasks(&self, filter: TaskListFilter) -> Result<TaskListPage, TaskError> {
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT
                id, name, input, state, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
                last_error, identity, locked_by, leased_until, ready_at, created_at,
                updated_at, completed_at
            FROM vyuh_tasks
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
            FROM vyuh_tasks
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(TaskError::from)
    }

    async fn run_migrations(&self) -> Result<(), TaskError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS vyuh_tasks (
                id BLOB PRIMARY KEY,
                name TEXT NOT NULL,
                input TEXT NOT NULL,
                state TEXT,
                resume_input TEXT,
                output TEXT,
                result TEXT,
                status INTEGER NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER,
                retry_delay_ms INTEGER,
                lease_duration_ms INTEGER,
                last_error TEXT,
                identity TEXT,
                locked_by TEXT,
                leased_until TEXT,
                ready_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        let has_priority: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM pragma_table_info('vyuh_tasks')
            WHERE name = 'priority'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        if has_priority == 0 {
            sqlx::query("ALTER TABLE vyuh_tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0")
                .execute(&self.pool)
                .await?;
        }

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_vyuh_tasks_pending_priority_claim
            ON vyuh_tasks(status, priority DESC, ready_at, created_at)
            WHERE status = 0
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_vyuh_tasks_running_lease
            ON vyuh_tasks(status, leased_until)
            WHERE status = 1
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS idx_vyuh_tasks_active_identity
            ON vyuh_tasks(identity)
            WHERE identity IS NOT NULL
              AND status IN (0, 1, 2)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_vyuh_tasks_name_status
            ON vyuh_tasks(name, status)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, Sqlite>, filter: &'a TaskListFilter) {
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
        let q = format!("%{}%", q.to_lowercase());
        builder.push(" AND (lower(name) LIKE ");
        builder.push_bind(q.clone());
        builder.push(" OR lower(identity) LIKE ");
        builder.push_bind(q.clone());
        builder.push(" OR lower(last_error) LIKE ");
        builder.push_bind(q);
        builder.push(")");
    }
}

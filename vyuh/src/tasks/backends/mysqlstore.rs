use crate::{
    db::Pool,
    tasks::{
        TaskError, TaskListFilter, TaskListPage, TaskOutcome, TaskRecord, TaskStatus,
        store::AbstractTaskStore,
    },
};
use sqlx::{MySql, QueryBuilder};

#[derive(Clone)]
pub struct MySqlTaskStore {
    pool: Pool,
    batch_size: usize,
    lease_duration: chrono::Duration,
}

impl MySqlTaskStore {
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
            if message.contains("idx_vyuh_tasks_identity") {
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
            UPDATE vyuh_tasks
            SET status = ?,
                state = COALESCE(?, state),
                resume_topic = ?,
                resume_input = ?,
                output = ?,
                result = ?,
                last_error = ?,
                ready_at = ?,
                completed_at = ?,
                attempts = attempts + CASE WHEN ? THEN 1 ELSE 0 END,
                locked_by = NULL,
                leased_until = NULL,
                updated_at = ?
            WHERE id = ?
              AND locked_by = ?
              AND status = ?
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

    async fn create_index_if_missing(&self, name: &str, sql: &str) -> Result<(), TaskError> {
        let exists: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM information_schema.statistics
            WHERE table_schema = DATABASE()
              AND table_name = 'vyuh_tasks'
              AND index_name = ?
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        if exists == 0 {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        Ok(())
    }

    async fn add_column_if_missing(&self, name: &str, sql: &str) -> Result<(), TaskError> {
        let exists: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM information_schema.columns
            WHERE table_schema = DATABASE()
              AND table_name = 'vyuh_tasks'
              AND column_name = ?
            "#,
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;

        if exists == 0 {
            sqlx::query(sql).execute(&self.pool).await?;
        }

        Ok(())
    }
}

impl AbstractTaskStore for MySqlTaskStore {
    async fn claim_tasks(&self, runner_id: &str) -> Result<Vec<TaskRecord>, TaskError> {
        let now = chrono::Utc::now();
        let default_lease_ms = self.lease_duration.num_milliseconds();
        let mut tx = self.pool.begin().await?;

        let ids: Vec<(uuid::Uuid,)> = sqlx::query_as(
            r#"
            SELECT id
            FROM vyuh_tasks
            WHERE (
                status = ?
                AND (ready_at IS NULL OR ready_at <= ?)
            ) OR (
                status = ?
                AND leased_until IS NOT NULL
                AND leased_until <= ?
            )
            ORDER BY
                priority DESC,
                CASE
                    WHEN status = ? THEN leased_until
                    ELSE COALESCE(ready_at, ?)
                END,
                created_at
            LIMIT ?
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(TaskStatus::Pending)
        .bind(now)
        .bind(TaskStatus::Running)
        .bind(now)
        .bind(TaskStatus::Running)
        .bind(now)
        .bind(self.batch_size as i64)
        .fetch_all(&mut *tx)
        .await?;

        let mut tasks = Vec::with_capacity(ids.len());

        for (id,) in &ids {
            let updated = sqlx::query(
                r#"
                UPDATE vyuh_tasks
                SET status = ?,
                    locked_by = ?,
                    leased_until = DATE_ADD(
                        ?,
                        INTERVAL (COALESCE(lease_duration_ms, ?) * 1000) MICROSECOND
                    ),
                    updated_at = ?
                WHERE id = ?
                  AND (
                    (
                        status = ?
                        AND (ready_at IS NULL OR ready_at <= ?)
                    ) OR (
                        status = ?
                        AND leased_until IS NOT NULL
                        AND leased_until <= ?
                    )
                  )
                "#,
            )
            .bind(TaskStatus::Running)
            .bind(runner_id)
            .bind(now)
            .bind(default_lease_ms)
            .bind(now)
            .bind(id)
            .bind(TaskStatus::Pending)
            .bind(now)
            .bind(TaskStatus::Running)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected();

            if updated > 0 {
                let task = sqlx::query_as::<_, TaskRecord>(
                    r#"
                    SELECT
                        id, name, input, state, resume_topic, resume_input, output, result,
                        status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
                        last_error, identity, locked_by, leased_until, ready_at, created_at,
                        updated_at, completed_at
                    FROM vyuh_tasks
                    WHERE id = ?
                      AND locked_by = ?
                      AND status = ?
                    "#,
                )
                .bind(id)
                .bind(runner_id)
                .bind(TaskStatus::Running)
                .fetch_one(&mut *tx)
                .await?;
                tasks.push(task);
            }
        }

        tx.commit().await?;
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
                let retry_delay_ms: Option<i64> = sqlx::query_scalar(
                    r#"
                    SELECT retry_delay_ms
                    FROM vyuh_tasks
                    WHERE id = ?
                      AND locked_by = ?
                      AND status = ?
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
                            THEN ? ELSE ? END,
                        attempts = attempts + 1,
                        last_error = ?,
                        ready_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN NULL ELSE ? END,
                        completed_at = CASE
                            WHEN max_attempts IS NOT NULL AND attempts + 1 >= max_attempts
                            THEN ? ELSE NULL END,
                        locked_by = NULL,
                        leased_until = NULL,
                        updated_at = ?
                    WHERE id = ?
                      AND locked_by = ?
                      AND status = ?
                    "#,
                )
                .bind(TaskStatus::Failed)
                .bind(TaskStatus::Pending)
                .bind(error)
                .bind(next_ready)
                .bind(now)
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
            INSERT INTO vyuh_tasks (
                id, name, input, state, resume_topic, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms, last_error, identity,
                locked_by, leased_until, ready_at, created_at, updated_at, completed_at
            )
            VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?
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

    async fn resume(&self, topic: &str, input: String) -> Result<u64, TaskError> {
        let now = chrono::Utc::now();
        let rows = sqlx::query(
            r#"
            UPDATE vyuh_tasks
            SET status = ?,
                resume_input = ?,
                resume_topic = NULL,
                ready_at = ?,
                updated_at = ?
            WHERE status = ?
              AND resume_topic = ?
            "#,
        )
        .bind(TaskStatus::Pending)
        .bind(input)
        .bind(now)
        .bind(now)
        .bind(TaskStatus::Suspended)
        .bind(topic)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows)
    }

    async fn list_tasks(&self, filter: TaskListFilter) -> Result<TaskListPage, TaskError> {
        let mut builder = QueryBuilder::<MySql>::new(
            r#"
            SELECT
                id, name, input, state, resume_topic, resume_input, output, result,
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
                id, name, input, state, resume_topic, resume_input, output, result,
                status, attempts, priority, max_attempts, retry_delay_ms, lease_duration_ms,
                last_error, identity, locked_by, leased_until, ready_at, created_at,
                updated_at, completed_at
            FROM vyuh_tasks
            WHERE id = ?
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
                id BINARY(16) PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                input TEXT NOT NULL,
                state TEXT,
                resume_topic VARCHAR(255),
                resume_input TEXT,
                output TEXT,
                result TEXT,
                status SMALLINT NOT NULL,
                attempts INT NOT NULL DEFAULT 0,
                priority INT NOT NULL DEFAULT 0,
                max_attempts INT,
                retry_delay_ms BIGINT,
                lease_duration_ms BIGINT,
                last_error TEXT,
                identity VARCHAR(255),
                active_identity VARCHAR(255)
                    GENERATED ALWAYS AS (
                        CASE
                            WHEN identity IS NOT NULL
                             AND status IN (0, 1, 2)
                            THEN identity
                            ELSE NULL
                        END
                    ) STORED,
                locked_by VARCHAR(64),
                leased_until DATETIME(6),
                ready_at DATETIME(6),
                created_at DATETIME(6) NOT NULL,
                updated_at DATETIME(6) NOT NULL,
                completed_at DATETIME(6)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing(
            "priority",
            "ALTER TABLE vyuh_tasks ADD COLUMN priority INT NOT NULL DEFAULT 0 AFTER attempts",
        )
        .await?;

        self.create_index_if_missing(
            "idx_vyuh_tasks_pending_priority_claim",
            r#"
            CREATE INDEX idx_vyuh_tasks_pending_priority_claim
            ON vyuh_tasks(status, priority DESC, ready_at, created_at)
            "#,
        )
        .await?;

        self.create_index_if_missing(
            "idx_vyuh_tasks_suspended_topic",
            r#"
            CREATE INDEX idx_vyuh_tasks_suspended_topic
            ON vyuh_tasks(status, resume_topic)
            "#,
        )
        .await?;

        self.create_index_if_missing(
            "idx_vyuh_tasks_running_lease",
            r#"
            CREATE INDEX idx_vyuh_tasks_running_lease
            ON vyuh_tasks(status, leased_until)
            "#,
        )
        .await?;

        self.create_index_if_missing(
            "idx_vyuh_tasks_identity",
            r#"
            CREATE UNIQUE INDEX idx_vyuh_tasks_identity
            ON vyuh_tasks(active_identity)
            "#,
        )
        .await?;

        self.create_index_if_missing(
            "idx_vyuh_tasks_name_status",
            r#"
            CREATE INDEX idx_vyuh_tasks_name_status
            ON vyuh_tasks(name, status)
            "#,
        )
        .await?;

        Ok(())
    }
}

fn push_filters<'a>(builder: &mut QueryBuilder<'a, MySql>, filter: &'a TaskListFilter) {
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

#![allow(async_fn_in_trait)]


use axum::http::StatusCode;
use axum::response::IntoResponse;
use thiserror::Error;

pub use sqlx::FromRow;
use sqlx::PgPool;
use sqlx::{self, Postgres, postgres::PgRow};
use sqlx::{Execute, IntoArguments};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing;

use crate::db::query::Statement;


#[derive(Debug)]
pub struct Notify {
    pub channel: String,
    pub payload: String,
}

#[derive(Debug)]
pub enum IntegrityKind {
    Unique,
    ForeignKey,
    Check,
    NotNull,
    Exclusion,
    Other(String),
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("integrity violation")]
    Integrity {
        kind: IntegrityKind,
        constraint: Option<String>,
        #[source]
        source: sqlx::Error,
    },
    #[error("query returned multiple rows")]
    MultipleObjects,
    #[error("record not found")]
    DoesNotExist,
    #[error("temporary database failure")]
    Temporary,
    #[error("bind error: {0}")]
    Bind(String),
    #[error("unhandled db error")]
    Fatal(sqlx::Error),
    #[error("bad query")]
    BadQuery,
}

impl DbError {
    pub const fn code(&self) -> &'static str {
        match self {
            DbError::Integrity { .. } => "integrity_violation",
            DbError::MultipleObjects => "multiple_objects",
            DbError::DoesNotExist => "not_found",
            DbError::Temporary => "temporary_error",
            DbError::Bind(_) => "bind_error",
            DbError::Fatal(_) => "fatal_error",
            DbError::BadQuery => "bad_query",
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            DbError::Integrity { .. } => StatusCode::CONFLICT,
            DbError::MultipleObjects => StatusCode::UNPROCESSABLE_ENTITY,
            DbError::DoesNotExist => StatusCode::NOT_FOUND,
            DbError::Temporary => StatusCode::SERVICE_UNAVAILABLE,
            DbError::Bind(_) => StatusCode::BAD_REQUEST,
            DbError::Fatal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            DbError::BadQuery => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => DbError::DoesNotExist,
            sqlx::Error::Database(db) => {
                let kind = match db.code().as_deref() {
                    Some("23505") => IntegrityKind::Unique,
                    Some("23503") => IntegrityKind::ForeignKey,
                    Some("23514") => IntegrityKind::Check,
                    Some("23502") => IntegrityKind::NotNull,
                    Some("23P01") => IntegrityKind::Exclusion,
                    c => IntegrityKind::Other(c.unwrap_or_default().into()),
                };
                DbError::Integrity {
                    kind,
                    constraint: db.constraint().map(|s| s.to_owned()),
                    source: e,
                }
            }
            sqlx::Error::Io(_) | sqlx::Error::Tls(_) => DbError::Temporary,
            _ => DbError::Fatal(e),
        }
    }
}

impl IntoResponse for DbError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let body = format!("Database error: {}", self);
        (status, body).into_response()
    }
}

pub trait DBSession {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError>;

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin;
        
    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin;

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin;

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Postgres> + sqlx::Type<Postgres> + Send + Unpin;

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError>;      
    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError>;

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError>;
}

pub struct DbTransaction<'a> {
    transaction: sqlx::Transaction<'a, Postgres>,
}

pub struct DbPool<'a> {
    pool: &'a PgPool,
}

impl<'a> DbPool<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn begin(&'a self) -> Result<DbTransaction<'a>, DbError> {
        let tx = self.pool.begin().await.map_err(|e| DbError::from(e))?;
        Ok(DbTransaction { transaction: tx })
    }

    pub(crate) async fn consume_notify(
        db_url: &str,
        topics: &[&str],
        capacity: usize,
    ) -> Result<mpsc::Receiver<Notify>, DbError> {
        let mut listener = sqlx::postgres::PgListener::connect(db_url)
            .await
            .map_err(|e| DbError::Fatal(e))?;
        for topic in topics {
            listener
                .listen(topic)
                .await
                .map_err(|e| DbError::Fatal(e))?;
        }

        let (sender, receiver) = mpsc::channel::<Notify>(capacity);

        tokio::spawn(async move {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        let notify = Notify {
                            channel: notification.channel().to_string(),
                            payload: notification.payload().to_string(),
                        };
                        match sender.try_send(notify) {
                            Ok(_) => {}
                            Err(TrySendError::Full(_)) => {
                                // Channel is full, skip this notification
                            }
                            Err(TrySendError::Closed(_)) => {
                                // Channel closed, exit the loop
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error receiving notification: {}", e);
                    }
                }
            }

            tracing::info!("Notification listener task ending");
        });

        Ok(receiver)
    }
}

impl DBSession for DbPool<'_> {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        let res = query.execute(self.pool).await.map_err(DbError::from)?;
        Ok(res.rows_affected())
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Postgres> + sqlx::Type<Postgres> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_scalar_with(&sql, args);
        query.fetch_one(self.pool).await.map_err(DbError::from)
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query.fetch_one(self.pool).await.map_err(DbError::from)
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query.fetch_all(self.pool).await.map_err(DbError::from)
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query.fetch_optional(self.pool).await.map_err(DbError::from)
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(self.pool, query, false).await
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(self.pool, query, true).await
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_all(self.pool, query).await
    }
}

impl DBSession for DbTransaction<'_> {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        let res = query
            .execute(&mut *self.transaction)
            .await
            .map_err(DbError::from)?;
        Ok(res.rows_affected())
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Postgres> + sqlx::Type<Postgres> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_scalar_with(&sql, args);
        query
            .fetch_one(&mut *self.transaction)
            .await
            .map_err(DbError::from)
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query
            .fetch_one(&mut *self.transaction)
            .await
            .map_err(DbError::from)
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query
            .fetch_all(&mut *self.transaction)
            .await
            .map_err(DbError::from)
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_as_with(&sql, args);
        query
            .fetch_optional(&mut *self.transaction)
            .await
            .map_err(DbError::from)
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&mut *self.transaction, query, false).await
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&mut *self.transaction, query, true).await
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts()?;
        let query = sqlx::query_with(&sql, args);
        jsql_all(&mut *self.transaction, query).await
    }
}

pub async fn jsql_get_strict<'e, 'q, A, E>(
    executor: E,
    mut query: sqlx::query::Query<'q, Postgres, A>,
    strict: bool,
) -> Result<String, DbError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{
    let sql_src = query.sql(); // &str
    let args = query
        .take_arguments()
        .unwrap_or_default()
        .unwrap_or_default();

    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql), '[]'::jsonb) FROM ({} ) jql",
        sql_src
    );

    let result: serde_json::Value = sqlx::query_scalar_with(wrapped_query.as_str(), args)
        .fetch_one(executor)
        .await
        .map_err(DbError::from)?;

    match result {
        serde_json::Value::Array(arr) => {
            if strict && arr.len() > 1 {
                Err(DbError::MultipleObjects)
            } else if arr.is_empty() {
                Err(DbError::DoesNotExist)
            } else {
                Ok(arr[0].to_string())
            }
        }
        _ => Err(DbError::Temporary),
    }
}

async fn jsql_all<'e, 'q, A, E>(
    executor: E,
    mut query: sqlx::query::Query<'q, Postgres, A>,
) -> Result<String, DbError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{
    let sql_src = query.sql(); // &str
    let args = query
        .take_arguments()
        .unwrap_or_default()
        .unwrap_or_default();

    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql)::text, '[]') FROM ({}) jql",
        sql_src
    );
    match sqlx::query_scalar_with(&wrapped_query, args)
        .fetch_one(executor)
        .await
    {
        Ok(result) => {
            let result: String = result;
            Ok(result)
        }
        Err(e) => return Err(DbError::from(e)),
    }
}

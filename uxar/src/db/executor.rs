#![allow(async_fn_in_trait)]

#[cfg(feature = "postgres")]
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tracing;

use crate::db::queries::{QueryError, Statement};
use crate::db::{Database, Pool, Row};
#[cfg(feature = "postgres")]
use crate::notifiers::CancellationNotifier;
use sqlx::{self, Execute, IntoArguments};

#[derive(Debug, Clone)]
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
    #[error("QuerySet error: {0}")]
    QuerySet(#[from] QueryError),
    #[error("unhandled db error")]
    Fatal(sqlx::Error),
    #[error("bad query")]
    BadQuery,
    #[error("feature not supported: {0}")]
    Unsupported(&'static str),
}

impl DbError {
    pub const fn code(&self) -> &'static str {
        match self {
            DbError::Integrity { .. } => "integrity_violation",
            DbError::MultipleObjects => "multiple_objects",
            DbError::DoesNotExist => "not_found",
            DbError::Temporary => "temporary_error",
            DbError::QuerySet(_) => "statement_error",
            DbError::Fatal(_) => "fatal_error",
            DbError::BadQuery => "bad_query",
            DbError::Unsupported(_) => "unsupported_feature",
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            DbError::Integrity { .. } => StatusCode::CONFLICT,
            DbError::MultipleObjects => StatusCode::UNPROCESSABLE_ENTITY,
            DbError::DoesNotExist => StatusCode::NOT_FOUND,
            DbError::Temporary => StatusCode::SERVICE_UNAVAILABLE,
            DbError::QuerySet(_) => StatusCode::BAD_REQUEST,
            DbError::Fatal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            DbError::BadQuery => StatusCode::INTERNAL_SERVER_ERROR,
            DbError::Unsupported(_) => StatusCode::NOT_IMPLEMENTED,
        }
    }
}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        match &e {
            sqlx::Error::RowNotFound => DbError::DoesNotExist,
            sqlx::Error::Database(db) => {
                #[cfg(feature = "postgres")]
                let kind = match db.code().as_deref() {
                    Some("23505") => IntegrityKind::Unique,
                    Some("23503") => IntegrityKind::ForeignKey,
                    Some("23514") => IntegrityKind::Check,
                    Some("23502") => IntegrityKind::NotNull,
                    Some("23P01") => IntegrityKind::Exclusion,
                    c => IntegrityKind::Other(c.unwrap_or_default().into()),
                };

                #[cfg(feature = "mysql")]
                let kind = match db.code().as_deref() {
                    Some("1062") => IntegrityKind::Unique,
                    Some("1451") | Some("1452") => IntegrityKind::ForeignKey,
                    Some("3819") => IntegrityKind::Check,
                    Some("1048") => IntegrityKind::NotNull,
                    c => IntegrityKind::Other(c.unwrap_or_default().into()),
                };

                #[cfg(feature = "sqlite")]
                let kind = match db.code().as_deref() {
                    Some("1555") | Some("2067") => IntegrityKind::Unique,
                    Some("787") => IntegrityKind::ForeignKey,
                    Some("275") => IntegrityKind::Check,
                    Some("1299") => IntegrityKind::NotNull,
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
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static;

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static;

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin + 'static;

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Database> + sqlx::Type<Database> + Send + Unpin + 'static;

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError>;
    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError>;

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError>;
}

pub struct DbTransaction<'a> {
    transaction: sqlx::Transaction<'a, Database>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConf {
    pub url: String,
    pub min_connections: u32,
    pub max_connections: u32,
    pub lazy: bool,    
}

impl Default for DbConf {
    /// Default configuration is always valid and zero-cost until first use.
    /// Uses feature-dependent URLs: sqlite::memory, postgres://localhost/test, or mysql://localhost/test
    fn default() -> Self {
        #[cfg(feature = "sqlite")]
        let url = "sqlite::memory:";
        
        #[cfg(all(feature = "postgres", not(feature = "sqlite")))]
        let url = "postgres://localhost/test";
        
        #[cfg(all(feature = "mysql", not(any(feature = "postgres", feature = "sqlite"))))]
        let url = "mysql://localhost/test";
        
        Self {
            url: url.into(),
            min_connections: 0,
            max_connections: 5,
            lazy: true,
        }
    }
}


impl DbConf {

    /// Load configuration from DATABASE_URL environment variable.
    /// Supports query parameters: max, min, lazy
    /// Example: postgres://user:pass@host/db?max=20&min=2&lazy=true
    pub fn from_env() -> Result<Self, DbError> {
        let url = std::env::var("DATABASE_URL")
            .map_err(|_| DbError::Fatal(
                sqlx::Error::Configuration("DATABASE_URL not set".into())
            ))?;

        Self::from_url(&url)
    }

    /// Parse configuration from a database URL string.
    /// Supports query parameters: max, min, lazy
    pub fn from_url(url: &str) -> Result<Self, DbError> {
        let parsed = url::Url::parse(url)
            .map_err(|e| DbError::Fatal(
                sqlx::Error::Configuration(format!("Invalid database URL: {}", e).into())
            ))?;

        let query_pairs: std::collections::HashMap<_, _> = parsed
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let max_connections = query_pairs
            .get("max")
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let min_connections = query_pairs
            .get("min")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let lazy = query_pairs
            .get("lazy")
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        // Remove query params from URL for the connection string
        let clean_url = Self::strip_query_params(url);

        Ok(Self {
            url: clean_url,
            min_connections,
            max_connections,
            lazy,
        })
    }

    fn strip_query_params(url: &str) -> String {
        url.split('?').next().unwrap_or(url).to_string()
    }
}


#[derive(Debug, Clone)]
pub struct DbPool {
    pool: Pool,
}

impl DbPool {

    #[inline]
    pub fn inner(&self) -> &Pool {
        &self.pool
    }

    pub fn from_pool(pool: Pool) -> Self {
        Self { pool }
    }

    pub async fn from_conf(conf: &DbConf)->Result<Self, DbError> {
        
        let builder = sqlx::pool::PoolOptions::<Database>::new()
            .min_connections(conf.min_connections)
            .max_connections(conf.max_connections);
        
        let pool = if conf.lazy {
            builder.connect_lazy(&conf.url)
                .map_err(|e| DbError::Fatal(e))?
        } else {
            builder.connect(&conf.url)
                .await
                .map_err(|e| DbError::Fatal(e))?
        };

        Ok(Self { pool })

    }

    pub async fn begin(&self) -> Result<DbTransaction<'_>, DbError> {
        let tx = self.pool.begin().await?;
        Ok(DbTransaction { transaction: tx })
    }

    pub async fn send_pgnotify(&self, channel: &str, payload: &str) -> Result<(), DbError> {
        #[cfg(feature = "postgres")]
        {
            let mut conn = self.pool.acquire().await?;
            let q = format!("NOTIFY {}, '{}'", channel, payload.replace('\'', "''"));
            let query = sqlx::query(&q);
            query.execute(&mut *conn).await?;
            Ok(())
        }
        #[cfg(not(feature = "postgres"))]
        {
            Err(DbError::Unsupported("PgNotify (Postgres only)"))
        }
    }

    /// Start listening to database notifications (Postgres only)
    #[cfg(feature = "postgres")]
    pub async fn consume_notify(
        &self,
        topics: &[String],
        capacity: usize,
        shutdown: CancellationNotifier,
    ) -> Result<mpsc::Receiver<Notify>, DbError> {
        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool).await?;
        for topic in topics {
            listener.listen(topic).await?;
        }

        let (sender, receiver) = mpsc::channel::<Notify>(capacity);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        tracing::info!("notification listener shutting down");
                        break;
                    }
                    notif = listener.recv() => {
                        match notif {
                            Ok(notification) => {
                                let notify = Notify {
                                    channel: notification.channel().into(),
                                    payload: notification.payload().into(),
                                };
                                if let Err(e) = sender.try_send(notify) {
                                    match e {
                                        TrySendError::Full(_) => continue,
                                        TrySendError::Closed(_) => break,
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("notification error: {}", e);
                            }
                        }
                    }
                }
            }
            tracing::info!("notification listener ended");
        });

        Ok(receiver)
    }

    #[cfg(not(feature = "postgres"))]
    pub(crate) async fn consume_notify(
        _: &str,
        _: &[&str],
        _: usize,
        _shutdown: Arc<tokio::sync::Notify>,
    ) -> Result<mpsc::Receiver<Notify>, DbError> {
        Err(DbError::Unsupported("LISTEN/NOTIFY (Postgres only)"))
    }

}

impl DBSession for DbPool {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        let res = query.execute(&self.pool).await?;
        Ok(res.rows_affected())
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Database> + sqlx::Type<Database> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_scalar_with(&sql, args);
        Ok(query.fetch_one(&self.pool).await?)
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_one(&self.pool).await?)
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_all(&self.pool).await?)
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_optional(&self.pool).await?)
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&self.pool, query, false).await
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&self.pool, query, true).await
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_all(&self.pool, query).await
    }
}

impl DBSession for DbTransaction<'_> {
    async fn execute(&mut self, qs: Statement) -> Result<u64, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        let res = query.execute(&mut *self.transaction).await?;
        Ok(res.rows_affected())
    }

    async fn fetch_scalar<T>(&mut self, qs: Statement) -> Result<T, DbError>
    where
        for<'d> T: sqlx::Decode<'d, Database> + sqlx::Type<Database> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_scalar_with(&sql, args);
        Ok(query.fetch_one(&mut *self.transaction).await?)
    }

    async fn fetch_one<M>(&mut self, qs: Statement) -> Result<M, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_one(&mut *self.transaction).await?)
    }

    async fn fetch_all<M>(&mut self, qs: Statement) -> Result<Vec<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_all(&mut *self.transaction).await?)
    }

    async fn fetch_optional<M>(&mut self, qs: Statement) -> Result<Option<M>, DbError>
    where
        M: for<'r> sqlx::FromRow<'r, Row> + Send + Unpin,
    {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_as_with(&sql, args);
        Ok(query.fetch_optional(&mut *self.transaction).await?)
    }

    async fn fetch_json_first(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&mut *self.transaction, query, false).await
    }

    async fn fetch_json_one(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_get_strict(&mut *self.transaction, query, true).await
    }

    async fn fetch_json_all(&mut self, qs: Statement) -> Result<String, DbError> {
        let (sql, args) = qs.into_parts().map_err(DbError::from)?;
        let query = sqlx::query_with(&sql, args);
        jsql_all(&mut *self.transaction, query).await
    }
}

async fn jsql_get_strict<'e, 'q, A, E>(
    executor: E,
    mut query: sqlx::query::Query<'q, Database, A>,
    strict: bool,
) -> Result<String, DbError>
where
    E: sqlx::Executor<'e, Database = Database>,
    for<'r> A: IntoArguments<'r, Database> + Default + Send,
{
    let sql_src = query.sql();
    let args = match query.take_arguments() {
        Ok(Some(args)) => args,
        Ok(None) => return Err(DbError::BadQuery),
        Err(e) => return Err(DbError::BadQuery),
    };

    #[cfg(feature = "postgres")]
    let wrapped = format!(
        "SELECT COALESCE(JSONB_AGG(jql), '[]'::jsonb) FROM ({}) jql",
        sql_src
    );

    #[cfg(feature = "mysql")]
    let wrapped = format!(
        "SELECT COALESCE(JSON_ARRAYAGG(JSON_OBJECT(*)), '[]') FROM ({}) jql",
        sql_src
    );

    #[cfg(feature = "sqlite")]
    let wrapped = format!(
        "SELECT COALESCE(json_group_array(json_object(*)), '[]') FROM ({}) jql",
        sql_src
    );

    let result: serde_json::Value = sqlx::query_scalar_with(&wrapped, args)
        .fetch_one(executor)
        .await?;

    let arr = result.as_array().ok_or(DbError::Temporary)?;

    if strict && arr.len() > 1 {
        return Err(DbError::MultipleObjects);
    }

    let first = arr.first().ok_or(DbError::DoesNotExist)?;
    Ok(first.to_string())
}

async fn jsql_all<'e, 'q, A, E>(
    executor: E,
    mut query: sqlx::query::Query<'q, Database, A>,
) -> Result<String, DbError>
where
    E: sqlx::Executor<'e, Database = Database>,
    for<'r> A: IntoArguments<'r, Database> + Default + Send,
{
    let sql_src = query.sql();
    let args = match query.take_arguments() {
        Ok(Some(args)) => args,
        Ok(None) => return Err(DbError::BadQuery),
        Err(e) => return Err(DbError::BadQuery),
    };

    #[cfg(feature = "postgres")]
    let wrapped = format!(
        "SELECT COALESCE(JSONB_AGG(jql)::text, '[]') FROM ({}) jql",
        sql_src
    );

    #[cfg(feature = "mysql")]
    let wrapped = format!(
        "SELECT COALESCE(JSON_ARRAYAGG(JSON_OBJECT(*)), '[]') FROM ({}) jql",
        sql_src
    );

    #[cfg(feature = "sqlite")]
    let wrapped = format!(
        "SELECT COALESCE(json_group_array(json_object(*)), '[]') FROM ({}) jql",
        sql_src
    );

    let result: String = sqlx::query_scalar_with(&wrapped, args)
        .fetch_one(executor)
        .await?;

    Ok(result)
}

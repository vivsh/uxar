use axum::http::StatusCode;
use sqlx::{self, postgres::{PgArguments, PgRow}, query::{Query, QueryAs}, FromRow, PgPool, Postgres};
use thiserror::Error;

use std::{fs, path::Path};
use serde_json::Value;


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
    #[error("unhandled db error")]
    Fatal(sqlx::Error),
}

impl DbError {
    pub const fn code(&self) -> &'static str {
        match self {
            DbError::Integrity { .. } => "integrity_violation",
            DbError::MultipleObjects => "multiple_objects",
            DbError::DoesNotExist => "not_found",
            DbError::Temporary => "temporary_error",
            DbError::Fatal(_) => "fatal_error",
        }
    }

    pub const fn status_code(&self) -> StatusCode {
        match self {
            DbError::Integrity { .. } => StatusCode::CONFLICT,
            DbError::MultipleObjects => StatusCode::UNPROCESSABLE_ENTITY,
            DbError::DoesNotExist => StatusCode::NOT_FOUND,
            DbError::Temporary => StatusCode::SERVICE_UNAVAILABLE,
            DbError::Fatal(_) => StatusCode::INTERNAL_SERVER_ERROR,
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

pub type PgQuery<'a> = Query<'a, Postgres, PgArguments>;
pub type PgQueryAs<'a, T: FromRow<'a, PgRow>> = QueryAs<'a, Postgres, T, PgArguments>;


#[derive(Clone)]
pub struct DbExecutor<'a> {
    pool: &'a PgPool
}

impl<'a> DbExecutor<'a> {

    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        self.pool
    }

    pub async fn execute(&self, query: PgQuery<'_>) -> Result<u64, DbError>
    {
        Ok(query.execute(self.pool)
                    .await
                    .map_err(DbError::from)?.rows_affected())
    }

    pub async fn fetch_one<T>(&self, query: PgQueryAs<'a, T>) -> Result<T, DbError>
    where
        T: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        query.fetch_one(self.pool)
            .await
            .map_err(DbError::from)
    }

    pub async fn fetch_all<T>(&self, query: PgQueryAs<'a, T>) -> Result<Vec<T>, DbError>
    where
        T: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        query.fetch_all(self.pool)
            .await
            .map_err(DbError::from)
    }

    pub async fn fetch_optional<T>(&self, query: PgQueryAs<'a, T>) -> Result<Option<T>, DbError>
    where
        T: for<'r> sqlx::FromRow<'r, PgRow> + Send + Unpin,
    {
        query.fetch_optional(self.pool)
            .await
            .map_err(DbError::from)
    }

}





/// A parsed SQL snippet with metadata.
#[derive(Debug)]
pub struct SqlTag {
    pub tag: String,
    pub meta: Option<Value>,
    pub sql: String,
}

/// Parse a SQL file into a `Vec<SqlSnippet>`.
pub fn extract_tags<P: AsRef<Path>>(path: P) -> anyhow::Result<Vec<SqlTag>> {
    let content = fs::read_to_string(path)?;
    let mut result = Vec::new();

    let mut current_tag = None::<String>;
    let mut current_meta = None::<Value>;
    let mut buffer = String::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("--@") {
            // flush previous snippet
            if let Some(tag) = current_tag.take() {
                result.push(SqlTag {
                    tag,
                    meta: current_meta.take(),
                    sql: buffer.trim().to_owned(),
                });
                buffer.clear();
            }

            // parse new tag + optional JSON
            let parts: Vec<&str> = rest.trim().splitn(2, char::is_whitespace).collect();
            current_tag = Some(parts[0].to_owned());

            if parts.len() > 1 {
                let maybe_json = parts[1].trim();
                if maybe_json.starts_with('{') {
                    match serde_json::from_str::<Value>(maybe_json) {
                        Ok(json) => current_meta = Some(json),
                        Err(e) => return Err(anyhow::anyhow!("Invalid JSON for tag {}: {}", parts[0], e)),
                    }
                }
            }
        } else if current_tag.is_some() {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    // flush last snippet
    if let Some(tag) = current_tag {
        result.push(SqlTag {
            tag,
            meta: current_meta,
            sql: buffer.trim().to_owned(),
        });
    }

    Ok(result)
}
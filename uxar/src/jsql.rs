use axum::response::IntoResponse;
use sqlx::{
    Arguments, IntoArguments, PgPool, Postgres, query::{Query}, Execute,
    postgres::PgArguments,
};

use crate::{db::DbError, errors::ToProblem};


pub async fn jsql_get<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
    strict: bool,
) -> axum::response::Response
where
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{
    jsql_get_strict(pool, query, true).await
}

pub async fn jsql_first<'q, A>(
    pool: &PgPool,
    query: Query<'q, Postgres, A>,
) -> axum::response::Response
where
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{
    jsql_get_strict(pool, query, false).await
}

pub async fn jsql_get_strict<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
    strict: bool,
) -> axum::response::Response
where
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{

    let sql_src = query.sql();                 // &str
    let args = query.take_arguments().unwrap_or_default().unwrap_or_default();

    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql), '[]'::jsonb) FROM ({} ) jql",
        sql_src
    );
    match sqlx::query_scalar_with(wrapped_query.as_str(), args)
        .fetch_one(pool)
        .await
    {
        Ok(result) => {
            let result: serde_json::Value = result;
            match result {
                serde_json::Value::Array(arr) => {
                    if strict && arr.len() > 1 {
                        return DbError::MultipleObjects.to_problem().into_response();
                    } else if strict && arr.is_empty() {
                        return DbError::DoesNotExist.to_problem().into_response();
                    } else {
                        let result = &arr[0];
                        return (
                            [(axum::http::header::CONTENT_TYPE, "application/json")],
                            result.to_string(),
                        )
                            .into_response();
                    }
                }
                _ => {
                    return DbError::Temporary.to_problem().into_response();
                }
            }
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal server error: {:?}", e),
        )
            .into_response(),
    }
}


pub async fn jsql_all<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
) -> axum::response::Response
where
    for<'r> A: IntoArguments<'r, Postgres> + Default + Send,
{
    let sql_src = query.sql();                 // &str
    let args = query.take_arguments().unwrap_or_default().unwrap_or_default();

    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql)::text, '[]') FROM ({}) jql",
        sql_src
    );
    match sqlx::query_scalar_with(&wrapped_query, args).fetch_one(pool).await {
        Ok(result) => {
            let result: String = result;
            (
                [(axum::http::header::CONTENT_TYPE, "application/json")],
                result,
            )
                .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal server error: {:?}", e),
        )
            .into_response(),
    }
}

pub struct Queryset {
    query: String,
    args: PgArguments,
}

impl Queryset {
    pub fn raw(sql: &str) -> Self {
        Self {
            query: sql.to_string(),
            args: PgArguments::default(),
        }
    }

    pub fn bind<T>(mut self, val: T) -> Self
    where
        T: for<'q> sqlx::Encode<'q, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + 'static,
    {
        self.args.add(val).unwrap(); // sqlx does the same so should be safe ??
        self
    }
}

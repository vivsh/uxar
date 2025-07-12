use axum::response::IntoResponse;
use sqlx::{
    Arguments, Database, IntoArguments, PgPool, Postgres, query::Query, QueryBuilder,
    postgres::PgArguments,
};

pub async fn jsql_get<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
    strict: bool,
) -> axum::response::Response
where
    A: Arguments<'q> + Send,
{
    jsql_get_strict(pool, query, true).await
}

pub async fn jsql_first<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
    strict: bool,
) -> axum::response::Response
where
    A: Arguments<'q> + Send,
{
    jsql_get_strict(pool, query, strict).await
}

async fn jsql_get_strict<'q, A>(
    pool: &PgPool,
    mut query: Query<'q, Postgres, A>,
    strict: bool,
) -> axum::response::Response
where
    A: Arguments<'q> + Send,
{
    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql), '[]'::jsonb) FROM ({} ) jql",
        query.sql()
    );
    match sqlx::query_scalar_with(wrapped_query.as_str(), query.take_arguments())
        .fetch_one(pool)
        .await
    {
        Ok(result) => {
            let result: serde_json::Value = result;
            match result {
                serde_json::Value::Array(arr) => {
                    if strict && arr.len() > 1 {
                        return (
                            axum::http::StatusCode::BAD_REQUEST,
                            "Query returned more than one row".to_string(),
                        )
                            .into_response();
                    } else if strict && arr.is_empty() {
                        return (
                            axum::http::StatusCode::NOT_FOUND,
                            "No results found".to_string(),
                        )
                            .into_response();
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
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Internal server error: {:?}", result),
                    )
                        .into_response();
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

pub async fn jsql_all(pool: &PgPool, query: &str) -> axum::response::Response {
    let wrapped_query = format!(
        "SELECT COALESCE(JSONB_AGG(jql)::text, '[]') FROM ({}) jql",
        query
    );
    match sqlx::query_scalar(&wrapped_query).fetch_one(pool).await {
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

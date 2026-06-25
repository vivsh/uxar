use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::{Row as _, sqlite::SqlitePoolOptions};

#[derive(Clone)]
struct AppState {
    pool: sqlx::SqlitePool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Echo {
    message: String,
    count: i64,
}

#[derive(Debug, Serialize)]
struct Health {
    ok: bool,
}

#[derive(Debug, Serialize)]
struct Item {
    id: i64,
    name: String,
}

async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

async fn echo(Json(input): Json<Echo>) -> Json<Echo> {
    Json(input)
}

async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Item>, axum::http::StatusCode> {
    let row = sqlx::query("SELECT id, name FROM items WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(row) = row else {
        return Err(axum::http::StatusCode::NOT_FOUND);
    };
    Ok(Json(Item {
        id: row
            .try_get("id")
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?,
        name: row
            .try_get("name")
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?,
    }))
}

async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO items (id, name) VALUES (1, 'alpha'), (2, 'beta'), (3, 'gamma')",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8000)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let db_url = std::env::var("T1_SQLITE_URL")
        .unwrap_or_else(|_| "sqlite:file:t1_axum?mode=memory&cache=shared".to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;
    migrate(&pool).await?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/echo", post(echo))
        .route("/items/{id}", get(get_item))
        .with_state(AppState { pool });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port())).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

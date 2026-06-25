use std::{
    convert::Infallible,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row as _, postgres::PgPoolOptions};
use tokio::sync::broadcast;
use tokio_stream::{StreamExt as _, wrappers::BroadcastStream};

#[derive(Clone)]
struct AppState {
    pool: sqlx::PgPool,
    tx: broadcast::Sender<LiveEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProjectIn {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct EventIn {
    value: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct PollQuery {
    after: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct LiveEvent {
    project_id: i64,
    kind: String,
    value: i64,
    at_ms: i64,
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true }))
}

async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ProjectIn>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query("INSERT INTO projects (name) VALUES ($1) RETURNING id")
        .bind(&input.name)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        json!({ "id": row.get::<i64, _>("id"), "name": input.name }),
    ))
}

async fn create_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(input): Json<EventIn>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query("INSERT INTO events (project_id, value) VALUES ($1, $2) RETURNING id")
        .bind(id)
        .bind(input.value)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let event = LiveEvent {
        project_id: id,
        kind: "event".to_string(),
        value: input.value,
        at_ms: now_ms(),
    };
    let _ = state.tx.send(event);
    Ok(Json(json!({ "id": row.get::<i64, _>("id") })))
}

async fn summary(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let row = sqlx::query("SELECT event_count, event_sum FROM rollups WHERE project_id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let value = row.map(|row| json!({ "project_id": id, "event_count": row.get::<i64, _>("event_count"), "event_sum": row.get::<i64, _>("event_sum") }))
        .unwrap_or_else(|| json!({ "project_id": id, "event_count": 0, "event_sum": 0 }));
    Ok(Json(value))
}

async fn events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<serde_json::Value>>, axum::http::StatusCode> {
    let rows = sqlx::query(
        "SELECT id, value FROM events WHERE project_id = $1 ORDER BY id DESC LIMIT 100",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(
        rows.into_iter()
            .map(
                |row| json!({ "id": row.get::<i64, _>("id"), "value": row.get::<i64, _>("value") }),
            )
            .collect(),
    ))
}

async fn stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.tx.subscribe()).filter_map(move |event| {
        let item = event.ok().filter(|event| event.project_id == id);
        item.and_then(|event| serde_json::to_string(&event).ok())
            .map(|data| Ok(Event::default().data(data)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn poll(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Query(query): Query<PollQuery>,
) -> Json<serde_json::Value> {
    let mut rx = state.tx.subscribe();
    let event = tokio::time::timeout(Duration::from_secs(25), async move {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.project_id == id {
                    return event;
                }
            }
        }
    })
    .await
    .ok();
    Json(json!({ "after": query.after, "events": event.into_iter().collect::<Vec<_>>() }))
}

async fn migrate(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    for sql in include_str!("../../sql/t2_postgres.sql").split(';') {
        let stmt = sql.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt).execute(pool).await?;
        }
    }
    Ok(())
}

fn spawn_rollup(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Ok(rows) = sqlx::query(
                "INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
                 SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now() FROM events GROUP BY project_id
                 ON CONFLICT (project_id) DO UPDATE SET event_count = EXCLUDED.event_count, event_sum = EXCLUDED.event_sum, updated_at = EXCLUDED.updated_at
                 RETURNING project_id, event_count",
            )
            .fetch_all(&state.pool)
            .await {
                for row in rows {
                    let _ = state.tx.send(LiveEvent { project_id: row.get("project_id"), kind: "rollup".to_string(), value: row.get("event_count"), at_ms: now_ms() });
                }
            }
        }
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
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
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/vyuh_bench".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;
    migrate(&pool).await?;
    let (tx, _) = broadcast::channel(10_000);
    let state = Arc::new(AppState { pool, tx });
    spawn_rollup(state.clone());
    let app = Router::new()
        .route("/health", get(health))
        .route("/projects", post(create_project))
        .route("/projects/{id}/events", post(create_event).get(events))
        .route("/projects/{id}/summary", get(summary))
        .route("/projects/{id}/stream", get(stream))
        .route("/projects/{id}/poll", get(poll))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port())).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

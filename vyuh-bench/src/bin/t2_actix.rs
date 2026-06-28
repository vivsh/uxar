use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use actix_web::{App, Error as ActixError, HttpResponse, HttpServer, Responder, get, post, web};
use async_stream::stream;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::broadcast;

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
struct Health {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct ProjectOut {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct IdOut {
    id: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct EventOut {
    id: i64,
    value: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
struct RollupRow {
    project_id: i64,
    event_count: i64,
    event_sum: i64,
}

#[derive(Debug, Clone, Serialize)]
struct SummaryOut {
    project_id: i64,
    event_count: i64,
    event_sum: i64,
}

#[derive(Debug, Clone, Serialize)]
struct PollOut {
    after: Option<u64>,
    events: Vec<LiveEvent>,
}

#[derive(Debug, Clone, Serialize)]
struct LiveEvent {
    project_id: i64,
    kind: String,
    value: i64,
    at_ms: i64,
}

#[get("/health")]
async fn health() -> impl Responder {
    web::Json(Health { ok: true })
}

#[post("/projects")]
async fn create_project(
    state: web::Data<Arc<AppState>>,
    input: web::Json<ProjectIn>,
) -> actix_web::Result<HttpResponse> {
    let project = sqlx::query_as::<_, ProjectOut>(
        "INSERT INTO projects (name) VALUES ($1) RETURNING id, name",
    )
    .bind(&input.name)
    .fetch_one(&state.pool)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().json(project))
}

#[post("/projects/{id}/events")]
async fn create_event(
    state: web::Data<Arc<AppState>>,
    id: web::Path<i64>,
    input: web::Json<EventIn>,
) -> actix_web::Result<HttpResponse> {
    let id = id.into_inner();
    let saved = sqlx::query_as::<_, IdOut>(
        "INSERT INTO events (project_id, value) VALUES ($1, $2) RETURNING id",
    )
    .bind(id)
    .bind(input.value)
    .fetch_one(&state.pool)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    let _ = state.tx.send(LiveEvent {
        project_id: id,
        kind: "event".to_string(),
        value: input.value,
        at_ms: now_ms(),
    });
    Ok(HttpResponse::Ok().json(saved))
}

#[get("/projects/{id}/summary")]
async fn summary(
    state: web::Data<Arc<AppState>>,
    id: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let id = id.into_inner();
    let row = sqlx::query_as::<_, RollupRow>(
        "SELECT project_id, event_count, event_sum FROM rollups WHERE project_id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    let value = row.map_or(
        SummaryOut {
            project_id: id,
            event_count: 0,
            event_sum: 0,
        },
        |row| SummaryOut {
            project_id: row.project_id,
            event_count: row.event_count,
            event_sum: row.event_sum,
        },
    );
    Ok(HttpResponse::Ok().json(value))
}

#[get("/projects/{id}/events")]
async fn events(
    state: web::Data<Arc<AppState>>,
    id: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let data = sqlx::query_as::<_, EventOut>(
        "SELECT id, value FROM events WHERE project_id = $1 ORDER BY id DESC LIMIT 100",
    )
    .bind(id.into_inner())
    .fetch_all(&state.pool)
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok().json(data))
}

#[get("/projects/{id}/stream")]
async fn sse(state: web::Data<Arc<AppState>>, id: web::Path<i64>) -> HttpResponse {
    let id = id.into_inner();
    let mut rx = state.tx.subscribe();
    let body = stream! {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.project_id == id {
                    if let Ok(data) = serde_json::to_string(&event) {
                        yield Ok::<_, ActixError>(web::Bytes::from(format!("data: {data}\n\n")));
                    }
                }
            }
        }
    };
    HttpResponse::Ok()
        .insert_header(("content-type", "text/event-stream"))
        .streaming(body)
}

#[get("/projects/{id}/poll")]
async fn poll(
    state: web::Data<Arc<AppState>>,
    id: web::Path<i64>,
    query: web::Query<PollQuery>,
) -> impl Responder {
    let id = id.into_inner();
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
    web::Json(PollOut {
        after: query.after,
        events: event.into_iter().collect(),
    })
}

fn spawn_rollup(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            if let Ok(rows) = sqlx::query_as::<_, RollupRow>(
                "INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
                 SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now() FROM events GROUP BY project_id
                 ON CONFLICT (project_id) DO UPDATE SET event_count = EXCLUDED.event_count, event_sum = EXCLUDED.event_sum, updated_at = EXCLUDED.updated_at
                 RETURNING project_id, event_count, event_sum",
            )
            .fetch_all(&state.pool)
            .await {
                for row in rows {
                    let _ = state.tx.send(LiveEvent { project_id: row.project_id, kind: "rollup".to_string(), value: row.event_count, at_ms: now_ms() });
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/vyuh_bench".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .map_err(std::io::Error::other)?;
    let (tx, _) = broadcast::channel(10_000);
    let state = Arc::new(AppState { pool, tx });
    spawn_rollup(state.clone());
    let state = web::Data::new(state);
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(health)
            .service(create_project)
            .service(create_event)
            .service(summary)
            .service(events)
            .service(sse)
            .service(poll)
    })
    .bind(("127.0.0.1", port()))?
    .run()
    .await
}

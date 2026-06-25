#[macro_use]
extern crate rocket;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rocket::{
    State,
    http::{ContentType, Status},
    response::stream::{Event, EventStream},
    serde::{
        Deserialize, Serialize,
        json::{Json, Value, json},
    },
};
use sqlx::{Row as _, postgres::PgPoolOptions};
use tokio::sync::broadcast;

struct AppState {
    pool: sqlx::PgPool,
    tx: broadcast::Sender<LiveEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct ProjectIn {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct EventIn {
    value: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(crate = "rocket::serde")]
struct LiveEvent {
    project_id: i64,
    kind: String,
    value: i64,
    at_ms: i64,
}

#[get("/health")]
fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[post("/projects", data = "<input>")]
async fn create_project(
    state: &State<Arc<AppState>>,
    input: Json<ProjectIn>,
) -> Result<Json<Value>, Status> {
    let row = sqlx::query("INSERT INTO projects (name) VALUES ($1) RETURNING id")
        .bind(&input.name)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(Json(
        json!({ "id": row.get::<i64, _>("id"), "name": input.name }),
    ))
}

#[post("/projects/<id>/events", data = "<input>")]
async fn create_event(
    state: &State<Arc<AppState>>,
    id: i64,
    input: Json<EventIn>,
) -> Result<Json<Value>, Status> {
    let row = sqlx::query("INSERT INTO events (project_id, value) VALUES ($1, $2) RETURNING id")
        .bind(id)
        .bind(input.value)
        .fetch_one(&state.pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    let _ = state.tx.send(LiveEvent {
        project_id: id,
        kind: "event".to_string(),
        value: input.value,
        at_ms: now_ms(),
    });
    Ok(Json(json!({ "id": row.get::<i64, _>("id") })))
}

#[get("/projects/<id>/summary")]
async fn summary(state: &State<Arc<AppState>>, id: i64) -> Result<Json<Value>, Status> {
    let row = sqlx::query("SELECT event_count, event_sum FROM rollups WHERE project_id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    let value = row.map(|row| json!({ "project_id": id, "event_count": row.get::<i64, _>("event_count"), "event_sum": row.get::<i64, _>("event_sum") }))
        .unwrap_or_else(|| json!({ "project_id": id, "event_count": 0, "event_sum": 0 }));
    Ok(Json(value))
}

#[get("/projects/<id>/events")]
async fn events(state: &State<Arc<AppState>>, id: i64) -> Result<Json<Vec<Value>>, Status> {
    let rows = sqlx::query(
        "SELECT id, value FROM events WHERE project_id = $1 ORDER BY id DESC LIMIT 100",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|_| Status::InternalServerError)?;
    Ok(Json(
        rows.into_iter()
            .map(
                |row| json!({ "id": row.get::<i64, _>("id"), "value": row.get::<i64, _>("value") }),
            )
            .collect(),
    ))
}

#[get("/projects/<id>/stream")]
fn sse(state: &State<Arc<AppState>>, id: i64) -> (ContentType, EventStream![]) {
    let mut rx = state.tx.subscribe();
    let stream = EventStream! {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.project_id == id {
                    yield Event::json(&event);
                }
            }
        }
    };
    (ContentType::EventStream, stream)
}

#[get("/projects/<id>/poll?<after>")]
async fn poll(state: &State<Arc<AppState>>, id: i64, after: Option<u64>) -> Json<Value> {
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
    Json(json!({ "after": after, "events": event.into_iter().collect::<Vec<_>>() }))
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

#[rocket::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let figment = rocket::Config::figment().merge(("port", port()));
    rocket::custom(figment)
        .manage(state)
        .mount(
            "/",
            routes![
                health,
                create_project,
                create_event,
                summary,
                events,
                sse,
                poll
            ],
        )
        .launch()
        .await?;
    Ok(())
}

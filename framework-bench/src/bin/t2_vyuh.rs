use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sqlx::Row as _;
use vyuh::{channels::ChannelCursor, db::DbConf, emitters::IterCount, prelude::*};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ProjectIn {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct EventIn {
    value: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ProjectPath {
    id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PollQuery {
    after: Option<ChannelCursor>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Health {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LiveEvent {
    project_id: i64,
    kind: String,
    value: i64,
    at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RollupTick {
    count: usize,
}

#[bundles::route(path = "/health")]
async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

#[bundles::route(path = "/projects", method = "POST")]
async fn create_project(
    site: Site,
    Json(input): Json<ProjectIn>,
) -> Result<Json<serde_json::Value>, Error> {
    let row = sqlx::query("INSERT INTO projects (name) VALUES ($1) RETURNING id")
        .bind(&input.name)
        .fetch_one(site.db().as_sqlx())
        .await?;
    let id: i64 = row.try_get("id")?;
    Ok(Json(json!({ "id": id, "name": input.name })))
}

#[bundles::route(path = "/projects/{id}/events", method = "POST")]
async fn create_event(
    site: Site,
    channels: ChannelRef,
    Path(ProjectPath { id }): Path<ProjectPath>,
    Json(input): Json<EventIn>,
) -> Result<Json<serde_json::Value>, Error> {
    let row = sqlx::query("INSERT INTO events (project_id, value) VALUES ($1, $2) RETURNING id")
        .bind(id)
        .bind(input.value)
        .fetch_one(site.db().as_sqlx())
        .await?;
    let event_id: i64 = row.try_get("id")?;
    let event = LiveEvent {
        project_id: id,
        kind: "event".to_string(),
        value: input.value,
        at_ms: now_ms(),
    };
    channels.publish(topic(id), Data::new(event)).await?;
    Ok(Json(json!({ "id": event_id })))
}

#[bundles::route(path = "/projects/{id}/summary")]
async fn summary(
    site: Site,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<Json<serde_json::Value>, Error> {
    let row =
        sqlx::query("SELECT event_count, event_sum, updated_at FROM rollups WHERE project_id = $1")
            .bind(id)
            .fetch_optional(site.db().as_sqlx())
            .await?;
    let Some(row) = row else {
        return Ok(Json(
            json!({ "project_id": id, "event_count": 0, "event_sum": 0 }),
        ));
    };
    Ok(Json(json!({
        "project_id": id,
        "event_count": row.try_get::<i64, _>("event_count")?,
        "event_sum": row.try_get::<i64, _>("event_sum")?
    })))
}

#[bundles::route(path = "/projects/{id}/events")]
async fn events(
    site: Site,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<Json<Vec<serde_json::Value>>, Error> {
    let rows = sqlx::query(
        "SELECT id, value FROM events WHERE project_id = $1 ORDER BY id DESC LIMIT 100",
    )
    .bind(id)
    .fetch_all(site.db().as_sqlx())
    .await?;
    let events = rows
        .into_iter()
        .map(|row| json!({ "id": row.get::<i64, _>("id"), "value": row.get::<i64, _>("value") }))
        .collect();
    Ok(Json(events))
}

#[bundles::route(path = "/projects/{id}/stream")]
async fn stream(
    channels: ChannelRef,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<ChannelSse, Error> {
    channels.sse(topic(id)).await.map_err(Error::from)
}

#[bundles::route(path = "/projects/{id}/poll")]
async fn poll(
    channels: ChannelRef,
    Path(ProjectPath { id }): Path<ProjectPath>,
    Query(query): Query<PollQuery>,
) -> Result<ChannelLongPoll, Error> {
    channels
        .long_poll(topic(id), query.after)
        .await
        .map_err(Error::from)
}

#[bundles::periodic(secs = 5)]
async fn rollup_tick(IterCount(count): IterCount) -> Data<RollupTick> {
    Data::new(RollupTick { count })
}

#[bundles::signal]
async fn run_rollup(
    site: Site,
    channels: ChannelRef,
    Data(_tick): Data<RollupTick>,
) -> Result<(), Error> {
    let rows = sqlx::query(
        "INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
         SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now()
         FROM events GROUP BY project_id
         ON CONFLICT (project_id) DO UPDATE SET
           event_count = EXCLUDED.event_count,
           event_sum = EXCLUDED.event_sum,
           updated_at = EXCLUDED.updated_at
         RETURNING project_id, event_count",
    )
    .fetch_all(site.db().as_sqlx())
    .await?;
    for row in rows {
        let id = row.try_get::<i64, _>("project_id")?;
        let count = row.try_get::<i64, _>("event_count")?;
        let event = LiveEvent {
            project_id: id,
            kind: "rollup".to_string(),
            value: count,
            at_ms: now_ms(),
        };
        channels.publish(topic(id), Data::new(event)).await?;
    }
    Ok(())
}

async fn migrate(site: &Site) -> Result<(), Error> {
    for sql in include_str!("../../sql/t2_postgres.sql").split(';') {
        let stmt = sql.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt).execute(site.db().as_sqlx()).await?;
        }
    }
    Ok(())
}

fn topic(id: i64) -> String {
    format!("projects.{id}")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
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
    let bundle = bundles::bundle! {
        health, create_project, create_event, summary, events, stream, poll, rollup_tick, run_rollup,
    };
    let conf = SiteConf::default()
        .host("127.0.0.1")
        .port(port())
        .secret_key("framework-bench-secret-key-32-bytes")
        .database(DbConf::from_url(&db_url)?);
    let site = Site::build(conf, bundle).await?;
    migrate(&site).await?;
    site.start().await?;
    Ok(())
}

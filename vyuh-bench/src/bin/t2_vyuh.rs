use std::time::{SystemTime, UNIX_EPOCH};

use vyuh::{db, db::DbConf, db::FilteredBuilder, emitters::IterCount, prelude::*};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ProjectIn {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct EventIn {
    value: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, vyuh::db::Bindable)]
struct NewProject {
    name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, vyuh::db::Bindable)]
struct NewEvent {
    project_id: i64,
    value: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct ProjectPath {
    id: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Health {
    ok: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema, vyuh::db::Scannable)]
struct ProjectOut {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema, vyuh::db::Scannable)]
struct IdOut {
    id: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema, vyuh::db::Scannable)]
struct EventOut {
    id: i64,
    value: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema, vyuh::db::Scannable)]
struct RollupRow {
    project_id: i64,
    event_count: i64,
    event_sum: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct SummaryOut {
    project_id: i64,
    event_count: i64,
    event_sum: i64,
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
) -> Result<Json<ProjectOut>, Error> {
    let mut session = site.db();
    let project = db::insert("projects")
        .row(&NewProject { name: input.name })
        .one::<ProjectOut, _>(&mut session)
        .await?;
    Ok(Json(project))
}

#[bundles::route(path = "/projects/{id}/events", method = "POST")]
async fn create_event(
    site: Site,
    Path(ProjectPath { id }): Path<ProjectPath>,
    Json(input): Json<EventIn>,
) -> Result<Json<IdOut>, Error> {
    let mut session = site.db();
    let saved = db::insert("events")
        .row(&NewEvent {
            project_id: id,
            value: input.value,
        })
        .one::<IdOut, _>(&mut session)
        .await?;
    let event = LiveEvent {
        project_id: id,
        kind: "event".to_string(),
        value: input.value,
        at_ms: now_ms(),
    };
    site.signals().emit(event).map_err(Error::other)?;
    Ok(Json(saved))
}

#[bundles::route(path = "/projects/{id}/summary")]
async fn summary(
    site: Site,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<Json<SummaryOut>, Error> {
    let mut session = site.db();
    let row = db::select("rollups")
        .filter("project_id = :id")
        .bind_as("id", id)
        .first::<RollupRow, _>(&mut session)
        .await?;
    Ok(Json(row.map_or(
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
    )))
}

#[bundles::route(path = "/projects/{id}/events")]
async fn events(
    site: Site,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<Json<Vec<EventOut>>, Error> {
    let mut session = site.db();
    let events = db::select("events")
        .filter("project_id = :id")
        .bind_as("id", id)
        .order_by("id", false)
        .slice(0, 100)
        .all::<EventOut, _>(&mut session)
        .await?;
    Ok(Json(events))
}

#[bundles::route(path = "/projects/{id}/stream")]
async fn stream(
    sub: Subscriber,
    channels: Channels,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<ChannelResponse, Error> {
    let stream = channels
        .user(UserKey::new(topic(id))?)
        .deliver_if::<LiveEvent, _>(move |event| event.project_id == id);
    sub.attach(stream).allow(SSE).await
}

#[bundles::route(path = "/projects/{id}/poll")]
async fn poll(
    sub: Subscriber,
    channels: Channels,
    Path(ProjectPath { id }): Path<ProjectPath>,
) -> Result<ChannelResponse, Error> {
    let stream = channels
        .user(UserKey::new(topic(id))?)
        .deliver_if::<LiveEvent, _>(move |event| event.project_id == id);
    sub.attach(stream).allow(POLL).await
}

#[bundles::periodic(secs = 5)]
async fn rollup_tick(IterCount(count): IterCount) -> Data<RollupTick> {
    Data::new(RollupTick { count })
}

#[bundles::signal]
async fn run_rollup(site: Site, Data(_tick): Data<RollupTick>) -> Result<(), Error> {
    let rows = sqlx::query_as::<_, RollupRow>(
        "INSERT INTO rollups (project_id, event_count, event_sum, updated_at)
         SELECT project_id, COUNT(*), COALESCE(SUM(value), 0), now()
         FROM events GROUP BY project_id
         ON CONFLICT (project_id) DO UPDATE SET
           event_count = EXCLUDED.event_count,
           event_sum = EXCLUDED.event_sum,
           updated_at = EXCLUDED.updated_at
         RETURNING project_id, event_count, event_sum",
    )
    .fetch_all(site.db().as_sqlx())
    .await?;
    for row in rows {
        let event = LiveEvent {
            project_id: row.project_id,
            kind: "rollup".to_string(),
            value: row.event_count,
            at_ms: now_ms(),
        };
        site.signals().emit(event).map_err(Error::other)?;
    }
    Ok(())
}

#[bundles::signal]
async fn audit_live_event(Data(_event): Data<LiveEvent>) {}

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
        audit_live_event,
    };
    let conf = SiteConf::default()
        .host("127.0.0.1")
        .port(port())
        .secret_key("vyuh-bench-secret-key-32-bytes-long")
        .database(DbConf::from_url(&db_url)?);
    Site::serve(conf, bundle).await.map_err(Into::into)
}

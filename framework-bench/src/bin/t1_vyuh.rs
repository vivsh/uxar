use sqlx::Row as _;
use vyuh::{db::DbConf, prelude::*};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
struct Echo {
    message: String,
    count: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Health {
    ok: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Item {
    id: i64,
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ItemPath {
    id: i64,
}

#[bundles::route(path = "/health")]
async fn health() -> Json<Health> {
    Json(Health { ok: true })
}

#[bundles::route(path = "/echo", method = "POST")]
async fn echo(Json(input): Json<Echo>) -> Json<Echo> {
    Json(input)
}

#[bundles::route(path = "/items/{id}")]
async fn get_item(site: Site, Path(ItemPath { id }): Path<ItemPath>) -> Result<Json<Item>, Error> {
    let row = sqlx::query("SELECT id, name FROM items WHERE id = ?")
        .bind(id)
        .fetch_optional(site.db().as_sqlx())
        .await?;
    let Some(row) = row else {
        return Err(Error::not_found("item not found"));
    };
    Ok(Json(Item {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
    }))
}

async fn migrate(site: &Site) -> Result<(), Error> {
    sqlx::query("CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .execute(site.db().as_sqlx())
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO items (id, name) VALUES (1, 'alpha'), (2, 'beta'), (3, 'gamma')",
    )
    .execute(site.db().as_sqlx())
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
        .unwrap_or_else(|_| "sqlite:file:t1_vyuh?mode=memory&cache=shared".to_string());
    let bundle = bundles::bundle! { health, echo, get_item };
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

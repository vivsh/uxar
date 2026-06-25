#[macro_use]
extern crate rocket;

use rocket::{
    State,
    http::Status,
    serde::{Deserialize, Serialize, json::Json},
};
use sqlx::{Row as _, sqlite::SqlitePoolOptions};

struct AppState {
    pool: sqlx::SqlitePool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
struct Echo {
    message: String,
    count: i64,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
struct Health {
    ok: bool,
}

#[derive(Debug, Serialize)]
#[serde(crate = "rocket::serde")]
struct Item {
    id: i64,
    name: String,
}

#[get("/health")]
fn health() -> Json<Health> {
    Json(Health { ok: true })
}

#[post("/echo", data = "<input>")]
fn echo(input: Json<Echo>) -> Json<Echo> {
    input
}

#[get("/items/<id>")]
async fn get_item(state: &State<AppState>, id: i64) -> Result<Json<Item>, Status> {
    let row = sqlx::query("SELECT id, name FROM items WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| Status::InternalServerError)?;
    let Some(row) = row else {
        return Err(Status::NotFound);
    };
    Ok(Json(Item {
        id: row.try_get("id").map_err(|_| Status::InternalServerError)?,
        name: row
            .try_get("name")
            .map_err(|_| Status::InternalServerError)?,
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

#[rocket::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_url = std::env::var("T1_SQLITE_URL")
        .unwrap_or_else(|_| "sqlite:file:t1_rocket?mode=memory&cache=shared".to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;
    migrate(&pool).await?;
    let figment = rocket::Config::figment().merge(("port", port()));
    rocket::custom(figment)
        .manage(AppState { pool })
        .mount("/", routes![health, echo, get_item])
        .launch()
        .await?;
    Ok(())
}

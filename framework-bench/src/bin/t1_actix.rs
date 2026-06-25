use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
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

#[get("/health")]
async fn health() -> impl Responder {
    web::Json(Health { ok: true })
}

#[post("/echo")]
async fn echo(input: web::Json<Echo>) -> impl Responder {
    web::Json(input.into_inner())
}

#[get("/items/{id}")]
async fn get_item(
    state: web::Data<AppState>,
    id: web::Path<i64>,
) -> actix_web::Result<HttpResponse> {
    let row = sqlx::query("SELECT id, name FROM items WHERE id = ?")
        .bind(id.into_inner())
        .fetch_optional(&state.pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let Some(row) = row else {
        return Ok(HttpResponse::NotFound().finish());
    };
    let item = Item {
        id: row
            .try_get("id")
            .map_err(actix_web::error::ErrorInternalServerError)?,
        name: row
            .try_get("name")
            .map_err(actix_web::error::ErrorInternalServerError)?,
    };
    Ok(HttpResponse::Ok().json(item))
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db_url = std::env::var("T1_SQLITE_URL")
        .unwrap_or_else(|_| "sqlite:file:t1_actix?mode=memory&cache=shared".to_string());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .map_err(std::io::Error::other)?;
    migrate(&pool).await.map_err(std::io::Error::other)?;
    let state = web::Data::new(AppState { pool });
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(health)
            .service(echo)
            .service(get_item)
    })
    .bind(("127.0.0.1", port()))?
    .run()
    .await
}

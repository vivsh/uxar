use actix_web::{App, HttpResponse, HttpServer, Responder, get, post, web};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqlitePoolOptions;

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

#[derive(Debug, Serialize, sqlx::FromRow)]
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
    let item = sqlx::query_as::<_, Item>("SELECT id, name FROM items WHERE id = ?")
        .bind(id.into_inner())
        .fetch_optional(&state.pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    let Some(item) = item else {
        return Ok(HttpResponse::NotFound().finish());
    };
    Ok(HttpResponse::Ok().json(item))
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

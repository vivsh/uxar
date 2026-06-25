#[macro_use] extern crate rocket;

use bcrypt::{DEFAULT_COST, hash, verify};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rocket::{Build, Rocket, State, http::Status, request::{FromRequest, Outcome, Request}, serde::json::Json};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::time::{SystemTime, UNIX_EPOCH};

const SECRET: &[u8] = b"change-me-in-production-32chars+";

#[derive(Serialize, Deserialize)]
struct Claims { sub: String, exp: u64 }

fn make_token(uid: i64) -> String {
    let exp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 86400;
    encode(&Header::default(), &Claims { sub: uid.to_string(), exp }, &EncodingKey::from_secret(SECRET)).unwrap()
}

fn verify_token(token: &str) -> Option<i64> {
    decode::<Claims>(token, &DecodingKey::from_secret(SECRET), &Validation::default())
        .ok().and_then(|d| d.claims.sub.parse().ok())
}

struct UserId(i64);
type Db = State<SqlitePool>;
type Res<T> = Result<Json<T>, (Status, Json<serde_json::Value>)>;

fn err(s: Status, msg: &str) -> (Status, Json<serde_json::Value>) {
    (s, Json(serde_json::json!({"error": msg})))
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UserId {
    type Error = ();
    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        match req.headers().get_one("Authorization").and_then(|v| v.strip_prefix("Bearer ")).and_then(verify_token) {
            Some(id) => Outcome::Success(UserId(id)),
            None => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
struct Thread { id: i64, title: String, author_id: i64, author: String, post_count: i64, created_at: String, updated_at: String }

#[derive(Serialize, sqlx::FromRow)]
struct Post { id: i64, thread_id: i64, author_id: i64, author: String, body: String, like_count: i64, created_at: String, updated_at: String }

#[derive(Deserialize)]
struct RegisterIn { username: String, email: String, password: String }

#[derive(Deserialize)]
struct LoginIn { username: String, password: String }

#[derive(Deserialize)]
struct ThreadIn { title: String }

#[derive(Deserialize)]
struct PostIn { body: String }

#[post("/auth/register", data = "<b>")]
async fn register(db: &Db, b: Json<RegisterIn>) -> Res<serde_json::Value> {
    let h = hash(&b.password, DEFAULT_COST).map_err(|_| err(Status::InternalServerError, "hash failed"))?;
    match sqlx::query("INSERT INTO users (username,email,password_hash) VALUES (?,?,?) RETURNING id")
        .bind(&b.username).bind(&b.email).bind(&h).fetch_one(db.inner()).await
    {
        Ok(r) => Ok(Json(serde_json::json!({"id": r.try_get::<i64,_>("id").unwrap(), "username": b.username}))),
        Err(e) if e.to_string().contains("UNIQUE") => Err(err(Status::Conflict, "Username or email taken")),
        Err(_) => Err(err(Status::InternalServerError, "DB error")),
    }
}

#[post("/auth/login", data = "<b>")]
async fn login(db: &Db, b: Json<LoginIn>) -> Res<serde_json::Value> {
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE username=? LIMIT 1")
        .bind(&b.username).fetch_optional(db.inner()).await.unwrap();
    let row = row.ok_or_else(|| err(Status::Unauthorized, "Invalid credentials"))?;
    let ph: String = row.try_get("password_hash").unwrap();
    if !verify(&b.password, &ph).unwrap_or(false) { return Err(err(Status::Unauthorized, "Invalid credentials")); }
    let uid: i64 = row.try_get("id").unwrap();
    Ok(Json(serde_json::json!({"access_token": make_token(uid), "token_type": "bearer"})))
}

#[get("/threads?<search>&<limit>&<offset>")]
async fn list_threads(db: &Db, search: Option<String>, limit: Option<i64>, offset: Option<i64>) -> Json<Vec<Thread>> {
    let s = search.as_deref().map(|v| format!("%{v}%"));
    let lim = limit.unwrap_or(50).min(100);
    let off = offset.unwrap_or(0);
    let rows = sqlx::query_as::<_, Thread>(
        "SELECT t.id, t.title, t.author_id, u.username AS author,
                (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                t.created_at, t.updated_at
         FROM threads t JOIN users u ON u.id=t.author_id
         WHERE (? IS NULL OR t.title LIKE ?) ORDER BY t.created_at DESC LIMIT ? OFFSET ?")
        .bind(&s).bind(&s).bind(lim).bind(off)
        .fetch_all(db.inner()).await.unwrap_or_default();
    Json(rows)
}

#[post("/threads", data = "<b>")]
async fn create_thread(db: &Db, user: UserId, b: Json<ThreadIn>) -> Res<serde_json::Value> {
    let row = sqlx::query("INSERT INTO threads (title, author_id) VALUES (?,?) RETURNING id")
        .bind(&b.title).bind(user.0).fetch_one(db.inner()).await.map_err(|_| err(Status::InternalServerError, "DB error"))?;
    Ok(Json(serde_json::json!({"id": row.try_get::<i64,_>("id").unwrap()})))
}

#[get("/threads/<id>")]
async fn get_thread(db: &Db, id: i64) -> Res<Thread> {
    let t = sqlx::query_as::<_, Thread>(
        "SELECT t.id, t.title, t.author_id, u.username AS author,
                (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                t.created_at, t.updated_at
         FROM threads t JOIN users u ON u.id=t.author_id WHERE t.id=?")
        .bind(id).fetch_optional(db.inner()).await.unwrap();
    t.map(Json).ok_or_else(|| err(Status::NotFound, "Thread not found"))
}

#[put("/threads/<id>", data = "<b>")]
async fn update_thread(db: &Db, user: UserId, id: i64, b: Json<ThreadIn>) -> Result<Status, (Status, Json<serde_json::Value>)> {
    let r = sqlx::query("UPDATE threads SET title=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?")
        .bind(&b.title).bind(id).bind(user.0).execute(db.inner()).await.unwrap();
    if r.rows_affected() == 0 { return Err(err(Status::Forbidden, "Not found or not your thread")); }
    Ok(Status::NoContent)
}

#[delete("/threads/<id>")]
async fn delete_thread(db: &Db, user: UserId, id: i64) -> Result<Status, (Status, Json<serde_json::Value>)> {
    let r = sqlx::query("DELETE FROM threads WHERE id=? AND author_id=?")
        .bind(id).bind(user.0).execute(db.inner()).await.unwrap();
    if r.rows_affected() == 0 { return Err(err(Status::Forbidden, "Not found or not your thread")); }
    Ok(Status::NoContent)
}

#[get("/threads/<tid>/posts?<limit>&<offset>")]
async fn list_posts(db: &Db, tid: i64, limit: Option<i64>, offset: Option<i64>) -> Json<Vec<Post>> {
    let rows = sqlx::query_as::<_, Post>(
        "SELECT p.id, p.thread_id, p.author_id, u.username AS author, p.body,
                (SELECT COUNT(*) FROM likes l WHERE l.post_id=p.id) AS like_count,
                p.created_at, p.updated_at
         FROM posts p JOIN users u ON u.id=p.author_id
         WHERE p.thread_id=? ORDER BY p.created_at ASC LIMIT ? OFFSET ?")
        .bind(tid).bind(limit.unwrap_or(50).min(100)).bind(offset.unwrap_or(0))
        .fetch_all(db.inner()).await.unwrap_or_default();
    Json(rows)
}

#[post("/threads/<tid>/posts", data = "<b>")]
async fn create_post(db: &Db, user: UserId, tid: i64, b: Json<PostIn>) -> Res<serde_json::Value> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM threads WHERE id=?")
        .bind(tid).fetch_optional(db.inner()).await.unwrap();
    if exists.is_none() { return Err(err(Status::NotFound, "Thread not found")); }
    let row = sqlx::query("INSERT INTO posts (thread_id, author_id, body) VALUES (?,?,?) RETURNING id")
        .bind(tid).bind(user.0).bind(&b.body).fetch_one(db.inner()).await.map_err(|_| err(Status::InternalServerError, "DB error"))?;
    Ok(Json(serde_json::json!({"id": row.try_get::<i64,_>("id").unwrap()})))
}

#[put("/posts/<id>", data = "<b>")]
async fn update_post(db: &Db, user: UserId, id: i64, b: Json<PostIn>) -> Result<Status, (Status, Json<serde_json::Value>)> {
    let r = sqlx::query("UPDATE posts SET body=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?")
        .bind(&b.body).bind(id).bind(user.0).execute(db.inner()).await.unwrap();
    if r.rows_affected() == 0 { return Err(err(Status::Forbidden, "Not found or not your post")); }
    Ok(Status::NoContent)
}

#[delete("/posts/<id>")]
async fn delete_post(db: &Db, user: UserId, id: i64) -> Result<Status, (Status, Json<serde_json::Value>)> {
    let r = sqlx::query("DELETE FROM posts WHERE id=? AND author_id=?")
        .bind(id).bind(user.0).execute(db.inner()).await.unwrap();
    if r.rows_affected() == 0 { return Err(err(Status::Forbidden, "Not found or not your post")); }
    Ok(Status::NoContent)
}

#[post("/posts/<id>/like")]
async fn add_like(db: &Db, user: UserId, id: i64) -> Status {
    sqlx::query("INSERT OR IGNORE INTO likes (post_id, user_id) VALUES (?,?)")
        .bind(id).bind(user.0).execute(db.inner()).await.ok();
    Status::NoContent
}

#[delete("/posts/<id>/like")]
async fn remove_like(db: &Db, user: UserId, id: i64) -> Status {
    sqlx::query("DELETE FROM likes WHERE post_id=? AND user_id=?")
        .bind(id).bind(user.0).execute(db.inner()).await.ok();
    Status::NoContent
}

#[get("/posts/<id>/like")]
async fn like_status(db: &Db, id: i64) -> Json<serde_json::Value> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE post_id=?")
        .bind(id).fetch_one(db.inner()).await.unwrap_or(0);
    Json(serde_json::json!({"count": count}))
}

async fn migrate(pool: &SqlitePool) {
    for sql in [
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL UNIQUE, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS threads (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS posts (id INTEGER PRIMARY KEY AUTOINCREMENT, thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS likes (post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, PRIMARY KEY (post_id, user_id))",
        "PRAGMA foreign_keys = ON",
    ] { sqlx::query(sql).execute(pool).await.unwrap(); }
}

#[launch]
async fn rocket() -> Rocket<Build> {
    let pool = SqlitePool::connect("sqlite:forum.db").await.expect("DB connect failed");
    migrate(&pool).await;
    rocket::build()
        .manage(pool)
        .mount("/", routes![
            register, login,
            list_threads, create_thread, get_thread, update_thread, delete_thread,
            list_posts, create_post, update_post, delete_post,
            add_like, remove_like, like_status,
        ])
}

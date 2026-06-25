use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sqlx::Row as _;
use vyuh::{
    ErrorKind,
    auth::{AuthUser, check_password, make_password},
    bundles,
    prelude::*,
};

#[derive(Debug, Serialize, JsonSchema)]
struct RegisterResp {
    id: i64,
    username: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct TokenResp {
    access_token: String,
    token_type: String,
}

#[derive(Debug, sqlx::FromRow)]
struct DbUser {
    id: i64,
    username: String,
    password_hash: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema, sqlx::FromRow)]
struct Thread {
    id: i64,
    title: String,
    author_id: i64,
    author: String,
    post_count: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema, sqlx::FromRow)]
struct Post {
    id: i64,
    thread_id: i64,
    author_id: i64,
    author: String,
    body: String,
    like_count: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RegisterReq {
    username: String,
    email: String,
    password: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct LoginReq {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ThreadIn {
    title: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PostIn {
    body: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct IdPath {
    id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ThreadPostPath {
    thread_id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListQuery {
    search: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn default_limit() -> i64 {
    50
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PageQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

#[derive(Debug, Serialize, JsonSchema)]
struct CreatedId {
    id: i64,
}

fn uid(user: &AuthUser) -> Result<i64, Error> {
    user.key
        .parse()
        .map_err(|_| Error::new(ErrorKind::Unauthorized))
}

#[bundles::route(path = "/auth/register", method = "POST")]
async fn register(site: Site, Json(req): Json<RegisterReq>) -> Result<Json<RegisterResp>, Error> {
    let hash = make_password(&req.password, None, None)?;
    let row = sqlx::query(
        "INSERT INTO users (username, email, password_hash) VALUES (?,?,?) RETURNING id",
    )
    .bind(&req.username)
    .bind(&req.email)
    .bind(&hash)
    .fetch_one(site.db().as_sqlx())
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.message().contains("UNIQUE") => {
            Error::new(ErrorKind::Conflict).with_context("Username or email taken")
        }
        _ => Error::from(e),
    })?;
    Ok(Json(RegisterResp {
        id: row.try_get("id")?,
        username: req.username,
    }))
}

#[bundles::route(path = "/auth/login", method = "POST")]
async fn login(site: Site, Json(req): Json<LoginReq>) -> Result<Json<TokenResp>, Error> {
    let user = sqlx::query_as::<_, DbUser>(
        "SELECT id, username, password_hash FROM users WHERE username=? LIMIT 1",
    )
    .bind(&req.username)
    .fetch_optional(site.db().as_sqlx())
    .await?;
    let user = user
        .ok_or_else(|| Error::new(ErrorKind::Unauthorized).with_context("Invalid credentials"))?;
    if !check_password(&req.password, &user.password_hash)? {
        return Err(Error::new(ErrorKind::Unauthorized).with_context("Invalid credentials"));
    }
    let pair = site
        .auth()
        .create_token_pair(AuthUser::new(&user.id.to_string(), 0), &[])?;
    Ok(Json(TokenResp {
        access_token: pair.access_token,
        token_type: "bearer".into(),
    }))
}

#[bundles::route(path = "/threads")]
async fn list_threads(site: Site, Query(q): Query<ListQuery>) -> Result<Json<Vec<Thread>>, Error> {
    let s = q.search.as_deref().map(|v| format!("%{v}%"));
    let rows = sqlx::query_as::<_, Thread>(
        "SELECT t.id, t.title, t.author_id, u.username AS author,
                (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                t.created_at, t.updated_at
         FROM threads t JOIN users u ON u.id=t.author_id
         WHERE (? IS NULL OR t.title LIKE ?) ORDER BY t.created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&s)
    .bind(&s)
    .bind(q.limit.min(100))
    .bind(q.offset)
    .fetch_all(site.db().as_sqlx())
    .await?;
    Ok(Json(rows))
}

#[bundles::route(path = "/threads", method = "POST")]
async fn create_thread(
    site: Site,
    user: AuthUser,
    Json(req): Json<ThreadIn>,
) -> Result<Json<CreatedId>, Error> {
    let row = sqlx::query("INSERT INTO threads (title, author_id) VALUES (?,?) RETURNING id")
        .bind(&req.title)
        .bind(uid(&user)?)
        .fetch_one(site.db().as_sqlx())
        .await?;
    Ok(Json(CreatedId {
        id: row.try_get("id")?,
    }))
}

#[bundles::route(path = "/threads/{id}")]
async fn get_thread(site: Site, Path(IdPath { id }): Path<IdPath>) -> Result<Json<Thread>, Error> {
    sqlx::query_as::<_, Thread>(
        "SELECT t.id, t.title, t.author_id, u.username AS author,
                (SELECT COUNT(*) FROM posts p WHERE p.thread_id=t.id) AS post_count,
                t.created_at, t.updated_at
         FROM threads t JOIN users u ON u.id=t.author_id WHERE t.id=?",
    )
    .bind(id)
    .fetch_optional(site.db().as_sqlx())
    .await?
    .map(Json)
    .ok_or_else(|| Error::new(ErrorKind::NotFound).with_context("Thread not found"))
}

#[bundles::route(path = "/threads/{id}", method = "PUT")]
async fn update_thread(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
    Json(req): Json<ThreadIn>,
) -> Result<StatusCode, Error> {
    let r = sqlx::query("UPDATE threads SET title=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?")
        .bind(&req.title).bind(id).bind(uid(&user)?).execute(site.db().as_sqlx()).await?;
    if r.rows_affected() == 0 {
        return Err(Error::new(ErrorKind::Forbidden).with_context("Not found or not your thread"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/threads/{id}", method = "DELETE")]
async fn delete_thread(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, Error> {
    let r = sqlx::query("DELETE FROM threads WHERE id=? AND author_id=?")
        .bind(id)
        .bind(uid(&user)?)
        .execute(site.db().as_sqlx())
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::new(ErrorKind::Forbidden).with_context("Not found or not your thread"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/threads/{thread_id}/posts")]
async fn list_posts(
    site: Site,
    Path(ThreadPostPath { thread_id }): Path<ThreadPostPath>,
    Query(q): Query<PageQuery>,
) -> Result<Json<Vec<Post>>, Error> {
    let rows = sqlx::query_as::<_, Post>(
        "SELECT p.id, p.thread_id, p.author_id, u.username AS author, p.body,
                (SELECT COUNT(*) FROM likes l WHERE l.post_id=p.id) AS like_count,
                p.created_at, p.updated_at
         FROM posts p JOIN users u ON u.id=p.author_id
         WHERE p.thread_id=? ORDER BY p.created_at ASC LIMIT ? OFFSET ?",
    )
    .bind(thread_id)
    .bind(q.limit.min(100))
    .bind(q.offset)
    .fetch_all(site.db().as_sqlx())
    .await?;
    Ok(Json(rows))
}

#[bundles::route(path = "/threads/{thread_id}/posts", method = "POST")]
async fn create_post(
    site: Site,
    user: AuthUser,
    Path(ThreadPostPath { thread_id }): Path<ThreadPostPath>,
    Json(req): Json<PostIn>,
) -> Result<Json<CreatedId>, Error> {
    let e: Option<i64> = sqlx::query_scalar("SELECT 1 FROM threads WHERE id=?")
        .bind(thread_id)
        .fetch_optional(site.db().as_sqlx())
        .await?;
    if e.is_none() {
        return Err(Error::new(ErrorKind::NotFound).with_context("Thread not found"));
    }
    let row =
        sqlx::query("INSERT INTO posts (thread_id, author_id, body) VALUES (?,?,?) RETURNING id")
            .bind(thread_id)
            .bind(uid(&user)?)
            .bind(&req.body)
            .fetch_one(site.db().as_sqlx())
            .await?;
    Ok(Json(CreatedId {
        id: row.try_get("id")?,
    }))
}

#[bundles::route(path = "/posts/{id}", method = "PUT")]
async fn update_post(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
    Json(req): Json<PostIn>,
) -> Result<StatusCode, Error> {
    let r = sqlx::query("UPDATE posts SET body=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? AND author_id=?")
        .bind(&req.body).bind(id).bind(uid(&user)?).execute(site.db().as_sqlx()).await?;
    if r.rows_affected() == 0 {
        return Err(Error::new(ErrorKind::Forbidden).with_context("Not found or not your post"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/posts/{id}", method = "DELETE")]
async fn delete_post(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, Error> {
    let r = sqlx::query("DELETE FROM posts WHERE id=? AND author_id=?")
        .bind(id)
        .bind(uid(&user)?)
        .execute(site.db().as_sqlx())
        .await?;
    if r.rows_affected() == 0 {
        return Err(Error::new(ErrorKind::Forbidden).with_context("Not found or not your post"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/posts/{id}/like", method = "POST")]
async fn add_like(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, Error> {
    sqlx::query("INSERT OR IGNORE INTO likes (post_id, user_id) VALUES (?,?)")
        .bind(id)
        .bind(uid(&user)?)
        .execute(site.db().as_sqlx())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/posts/{id}/like", method = "DELETE")]
async fn remove_like(
    site: Site,
    user: AuthUser,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<StatusCode, Error> {
    sqlx::query("DELETE FROM likes WHERE post_id=? AND user_id=?")
        .bind(id)
        .bind(uid(&user)?)
        .execute(site.db().as_sqlx())
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[bundles::route(path = "/posts/{id}/like")]
async fn like_status(
    site: Site,
    Path(IdPath { id }): Path<IdPath>,
) -> Result<Json<serde_json::Value>, Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM likes WHERE post_id=?")
        .bind(id)
        .fetch_one(site.db().as_sqlx())
        .await?;
    Ok(Json(serde_json::json!({"count": count})))
}

async fn migrate(site: &Site) -> Result<(), Error> {
    let p = site.db();
    for sql in [
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL UNIQUE, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS threads (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS posts (id INTEGER PRIMARY KEY AUTOINCREMENT, thread_id INTEGER NOT NULL REFERENCES threads(id) ON DELETE CASCADE, author_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')), updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')))",
        "CREATE TABLE IF NOT EXISTS likes (post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, PRIMARY KEY (post_id, user_id))",
        "PRAGMA foreign_keys = ON",
    ] {
        sqlx::query(sql).execute(p.as_sqlx()).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    let bundle = bundles::bundle! {
        register, login,
        list_threads, create_thread, get_thread, update_thread, delete_thread,
        list_posts, create_post, update_post, delete_post,
        add_like, remove_like, like_status,
    };
    let site = Site::build(SiteConf::default().port(8000), bundle)
        .await
        .expect("build failed");
    migrate(&site).await.expect("migrate failed");
    println!("Forum running on http://localhost:8000");
    site.start().await.expect("site stopped");
}

use axum::http::StatusCode;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    Site,
    auth::BitRole,
    console::{
        auth::{ConsoleCookies, ConsoleSessionUser, expired_cookie, session_cookie},
        query::{OperationQuery, TaskQuery, filter_operations},
        types::{OperationOut, Page, SessionOut, TaskDetailOut, TaskOut},
    },
    routes::{Json, Path, Query},
};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LoginQuery {
    token: String,
}

pub async fn login(
    site: Site,
    Query(query): Query<LoginQuery>,
    ConsoleCookies(jar): ConsoleCookies,
) -> Result<(axum_extra::extract::CookieJar, Json<serde_json::Value>), StatusCode> {
    let runtime = site.console_runtime().ok_or(StatusCode::NOT_FOUND)?;
    let conf = &site.conf().console;
    let ttl = std::time::Duration::from_secs(conf.bootstrap_token_ttl_seconds);
    let token = runtime
        .consume_bootstrap(&query.token, ttl)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let max_age = time::Duration::seconds(conf.bootstrap_token_ttl_seconds as i64);
    let jar = jar.add(session_cookie(&conf.cookie_name, token, max_age));
    Ok((
        jar,
        Json(serde_json::json!({
            "ok": true,
            "session": {
                "subject": "bootstrap",
                "roles": crate::console::ConsoleRole::Admin.to_role_type(),
                "role_names": ["admin"],
            }
        })),
    ))
}

pub async fn logout(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    ConsoleCookies(jar): ConsoleCookies,
) -> (axum_extra::extract::CookieJar, Json<serde_json::Value>) {
    let conf = &site.conf().console;
    if let Some(cookie) = jar.get(&conf.cookie_name)
        && let Some(runtime) = site.console_runtime()
    {
        runtime.clear_session(cookie.value());
    }
    let jar = jar.add(expired_cookie(&conf.cookie_name));
    (jar, Json(serde_json::json!({ "ok": true })))
}

pub async fn session(ConsoleSessionUser(user): ConsoleSessionUser) -> Json<SessionOut> {
    Json(SessionOut {
        subject: user.subject,
        roles: user.roles,
        role_names: user.role_names,
    })
}

pub async fn operations(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(query): Query<OperationQuery>,
) -> Json<Page<OperationOut>> {
    let conf = &site.conf().console;
    let (items, next_cursor) = filter_operations(
        site.iter_operations(),
        &query,
        conf.page_size_default,
        conf.page_size_max,
    );
    Json(Page {
        items: items.into_iter().map(OperationOut::from).collect(),
        next_cursor,
    })
}

pub async fn operation_detail(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(id): Path<String>,
) -> Result<Json<OperationOut>, StatusCode> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    site.iter_operations()
        .find(|op| op.id == id)
        .map(OperationOut::from)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub async fn tasks(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(query): Query<TaskQuery>,
) -> Result<Json<Page<TaskOut>>, StatusCode> {
    let conf = &site.conf().console;
    let filter = query.to_filter(conf.page_size_default, conf.page_size_max);
    let page = site
        .tasks()
        .list(filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(Page {
        items: page.records.iter().map(TaskOut::from).collect(),
        next_cursor: page.next_cursor,
    }))
}

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
pub async fn tasks(
    _site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(_query): Query<TaskQuery>,
) -> Result<Json<Page<TaskOut>>, StatusCode> {
    Ok(Json(Page {
        items: Vec::new(),
        next_cursor: None,
    }))
}

#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub async fn task_detail(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(id): Path<String>,
) -> Result<Json<TaskDetailOut>, StatusCode> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    let record = site
        .tasks()
        .get(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(TaskDetailOut::from(&record)))
}

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
pub async fn task_detail(
    _site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(_id): Path<String>,
) -> Result<Json<TaskDetailOut>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

pub async fn status(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
) -> Json<crate::console::status::StatusOut> {
    let ttl = std::time::Duration::from_secs(site.conf().console.status_cache_ttl_seconds);
    let status = site
        .console_runtime()
        .map(|runtime| runtime.status(&site, ttl))
        .unwrap_or_else(|| crate::console::status::collect(&site));
    Json(status)
}

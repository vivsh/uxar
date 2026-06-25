use axum::{http::StatusCode, response::Html};
use serde_json::json;

use crate::routes::{Path, Query};
use crate::{
    Site,
    console::{
        auth::ConsoleSessionUser,
        query::{OperationQuery, TaskQuery, filter_operations},
        types::{OperationOut, Page, TaskDetailOut, TaskOut},
    },
    templates::TemplateError,
};

pub async fn login(site: Site) -> Result<Html<String>, TemplateError> {
    render(
        &site,
        "console/login.html",
        json!({ "base_path": base_path(&site) }),
    )
}

pub async fn overview(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
) -> Result<Html<String>, TemplateError> {
    let ttl = std::time::Duration::from_secs(site.conf().console.status_cache_ttl_seconds);
    let status = site
        .console_runtime()
        .map(|runtime| runtime.status(&site, ttl))
        .unwrap_or_else(|| crate::console::status::collect(&site));
    render_page(
        &site,
        "console/overview.html",
        "overview",
        "Overview",
        json!({ "status": status }),
    )
}

pub async fn operations(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(query): Query<OperationQuery>,
) -> Result<Html<String>, TemplateError> {
    let conf = &site.conf().console;
    let (items, next_cursor) = filter_operations(
        site.iter_operations(),
        &query,
        conf.page_size_default,
        conf.page_size_max,
    );
    let page = Page {
        items: items
            .into_iter()
            .map(OperationOut::from)
            .collect::<Vec<_>>(),
        next_cursor,
    };
    let selected_operation = selected_operation(&site, &query);
    render_page(
        &site,
        "console/operations.html",
        "operations",
        "Operations",
        json!({
            "page": page,
            "query": query,
            "kinds": operation_kinds(),
            "selected_operation": selected_operation,
        }),
    )
}

pub async fn operation_detail(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    let operation = site
        .iter_operations()
        .find(|op| op.id == id)
        .map(OperationOut::from)
        .ok_or(StatusCode::NOT_FOUND)?;
    render_page(
        &site,
        "console/operation_detail.html",
        "operations",
        "Operation",
        json!({ "operation": operation }),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub async fn tasks(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(query): Query<TaskQuery>,
) -> Result<Html<String>, StatusCode> {
    if !site.console_has_tasks() {
        return render_tasks(&site, query, empty_tasks())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
    }

    let conf = &site.conf().console;
    let filter = query.to_filter(conf.page_size_default, conf.page_size_max);
    let page = site
        .tasks()
        .list(filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let page = Page {
        items: page.records.iter().map(TaskOut::from).collect::<Vec<_>>(),
        next_cursor: page.next_cursor,
    };
    render_tasks(&site, query, page).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
pub async fn tasks(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Query(query): Query<TaskQuery>,
) -> Result<Html<String>, StatusCode> {
    let page = Page::<TaskOut> {
        items: Vec::new(),
        next_cursor: None,
    };
    render_tasks(&site, query, page).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(any(feature = "postgres", feature = "mysql", feature = "sqlite"))]
pub async fn task_detail(
    site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let id = uuid::Uuid::parse_str(&id).map_err(|_| StatusCode::NOT_FOUND)?;
    let record = site
        .tasks()
        .get(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let detail = TaskDetailOut::from(&record);
    let payload =
        serde_json::to_string_pretty(&detail).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    render_page(
        &site,
        "console/task_detail.html",
        "tasks",
        "Task",
        json!({ "task": detail, "payload": payload }),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
pub async fn task_detail(
    _site: Site,
    ConsoleSessionUser(_user): ConsoleSessionUser,
    Path(_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    Err(StatusCode::NOT_FOUND)
}

fn render_page(
    site: &Site,
    template: &str,
    active: &str,
    title: &str,
    mut context: serde_json::Value,
) -> Result<Html<String>, TemplateError> {
    context["active"] = json!(active);
    context["title"] = json!(title);
    context["base_path"] = json!(base_path(site));
    render(site, template, context)
}

fn render(
    site: &Site,
    template: &str,
    context: serde_json::Value,
) -> Result<Html<String>, TemplateError> {
    site.template_engine().html(template, &context)
}

fn render_tasks(
    site: &Site,
    query: TaskQuery,
    page: Page<TaskOut>,
) -> Result<Html<String>, TemplateError> {
    render_page(
        site,
        "console/tasks.html",
        "tasks",
        "Tasks",
        json!({ "page": page, "query": query, "statuses": task_statuses() }),
    )
}

fn selected_operation(site: &Site, query: &OperationQuery) -> Option<OperationOut> {
    let id = query.selected.as_deref()?;
    let id = uuid::Uuid::parse_str(id).ok()?;
    site.iter_operations()
        .find(|op| op.id == id)
        .map(OperationOut::from)
}

fn empty_tasks() -> Page<TaskOut> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

fn base_path(site: &Site) -> &str {
    &site.conf().console.path
}

fn operation_kinds() -> [&'static str; 9] {
    [
        "route", "command", "task", "service", "signal", "cron", "periodic", "pgnotify", "api_doc",
    ]
}

fn task_statuses() -> [&'static str; 5] {
    ["pending", "running", "suspended", "succeeded", "failed"]
}

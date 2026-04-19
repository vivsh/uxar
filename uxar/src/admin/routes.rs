use crate::routes::{Html, IntoResponse, Response};
use crate::templates::TemplateError;
use crate::{Site, bundles, embed};
use axum::extract::FromRequestParts;
use rust_silos::{Silo, embed_silo};

const ADMIN_ASSETS: Silo = embed_silo!("src/admin/assets");


pub enum AdminError {
    TemplateError(TemplateError),
    LoginRequired,
}



impl AdminError {
 
}

impl axum::response::IntoResponse for AdminError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AdminError::LoginRequired => {
                // unauthenticated so redirect to login
                let response = axum::response::Response::builder()
                    .status(axum::http::StatusCode::TEMPORARY_REDIRECT)
                    .header(axum::http::header::LOCATION, "/v1/admin/login")
                    .body(axum::body::Body::empty())
                    .unwrap_or_else(|_| service_unavailable().into_response());
                response
            }
            AdminError::TemplateError(e) => {
                let body = format!("Template error: {}", e);
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
            }
        }
    }
}


fn service_unavailable() -> Response {
    let body = "<h1>503 Service Unavailable</h1><p>The service is temporarily unavailable. Please try again later.</p>";
    Html(body.to_string()).into_response()
}

fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "css"  => "text/css; charset=utf-8",
        "js"   => "application/javascript; charset=utf-8",
        "svg"  => "image/svg+xml",
        "png"  => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico"  => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf"  => "font/ttf",
        _      => "application/octet-stream",
    }
}


pub struct AdminAuth{

}

impl FromRequestParts<Site> for AdminAuth {
    type Rejection = AdminError;

    async fn from_request_parts(
        _parts: &mut axum::http::request::Parts,
        _state: &Site,
    ) -> Result<Self, Self::Rejection> {
        // Placeholder logic for authentication
        Ok(AdminAuth{})
    }
} 

fn render(
    site: &Site,
    template_name: &str,
    context: &serde_json::Value,
) -> Result<Html<String>, AdminError> {
    let content = site
        .render_template(template_name, context)
        .map_err(AdminError::TemplateError)?;
    Ok(Html(content))
}

#[bundles::route(path = "/greeting", method = "GET")]
async fn greeting() -> Response {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::TEMPORARY_REDIRECT)
        .header(axum::http::header::LOCATION, "/v1/admin/operations")
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| service_unavailable().into_response())
}

#[bundles::route(path = "/operations", method = "GET")]
async fn operations(site: Site) -> Result<Html<String>, AdminError> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut by_kind: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    let mut all_tags: BTreeSet<String> = BTreeSet::new();

    for op in site.iter_operations().filter(|op| !op.hidden) {
        let kind = format!("{:?}", op.kind);

        let methods: Vec<&str> = op.http_methods();

        for tag in &op.tags {
            all_tags.insert(tag.to_string());
        }

        let op_json = serde_json::json!({
            "name": op.name,
            "summary": op.summary,
            "description": op.description,
            "path": op.path,
            "methods": methods,
            "args": op.args.iter().map(|arg| {
                let (type_name, schema_json) = match &arg.part {
                    crate::callables::ArgPart::Header(ts) |
                    crate::callables::ArgPart::Cookie(ts) |
                    crate::callables::ArgPart::Query(ts) |
                    crate::callables::ArgPart::Path(ts) |
                    crate::callables::ArgPart::Body(ts, _) => {
                        let name = (ts.type_name)().to_string();
                        let json = serde_json::to_value(ts).ok()
                            .and_then(|v| serde_json::to_string(&v).ok());
                        (name, json)
                    },
                    crate::callables::ArgPart::Security { scheme, .. } => (scheme.to_string(), None),
                    crate::callables::ArgPart::Zone => ("Zone".to_string(), None),
                    crate::callables::ArgPart::Ignore => ("Ignored".to_string(), None),
                };
                serde_json::json!({
                    "name": arg.name,
                    "type_name": type_name,
                    "schema_json": schema_json,
                    "description": arg.description,
                })
            }).collect::<Vec<_>>(),
            "returns": op.returns.iter().map(|ret| {
                let (type_name, schema_json) = match &ret.part {
                    crate::callables::ReturnPart::Header(ts) |
                    crate::callables::ReturnPart::Body(ts, _) => {
                        let name = (ts.type_name)().to_string();
                        let json = serde_json::to_value(ts).ok()
                            .and_then(|v| serde_json::to_string(&v).ok());
                        (name, json)
                    },
                    crate::callables::ReturnPart::Empty => ("Empty".to_string(), None),
                    crate::callables::ReturnPart::Unknown => ("Unknown".to_string(), None),
                };
                serde_json::json!({
                    "status": ret.status_code,
                    "type_name": type_name,
                    "schema_json": schema_json,
                    "description": ret.description,
                })
            }).collect::<Vec<_>>(),
            "tags": op.tags,
        });

        by_kind.entry(kind).or_default().push(op_json);
    }

    let all_kinds: Vec<String> = by_kind.keys().cloned().collect();
    let operations_by_kind: Vec<_> = by_kind.into_iter().collect();

    let context = serde_json::json!({
        "operations_by_kind": operations_by_kind,
        "all_kinds": all_kinds,
        "all_tags": all_tags,
    });

    let content = render(&site, "admin/operations.html", &context)?;
    Ok(content)
}


#[bundles::asset_dir]
fn admin_assets_dir() -> embed::Dir {
    embed::Dir::new(ADMIN_ASSETS.clone())
}

#[bundles::route(path = "/static/{*path}", method = "GET")]
async fn static_file(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    let clean = path.trim_start_matches('/');
    if clean.is_empty() || clean.contains("..") {
        return (axum::http::StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let file_path = format!("static/{}", clean);
    let dir = embed::Dir::new(ADMIN_ASSETS.clone());
    match dir.get_file(&file_path) {
        Some(file) => match file.read_bytes_sync() {
            Ok(bytes) => (
                [(axum::http::header::CONTENT_TYPE, content_type_for(&file_path))],
                bytes,
            ).into_response(),
            Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Read error").into_response(),
        },
        None => (axum::http::StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}





#[bundles::route(path = "/login")]
async fn login_view(site: Site) -> Result<Html<String>, AdminError> {
    let context = serde_json::json!({ "current_page": "login" });
    render(&site, "admin/login.html", &context)
}


#[bundles::route(path = "/conf")]
async fn conf_view(site: Site) -> Result<Html<String>, AdminError> {
    let conf = site.conf();
    let db_url_masked = {
        let url = &conf.database.url;
        // mask password in URLs like postgres://user:pass@host/db
        if let Some(at) = url.find('@') {
            if let Some(colon) = url[..at].rfind(':') {
                let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
                if colon > scheme_end {
                    format!("{}:***@{}", &url[..colon], &url[at + 1..])
                } else {
                    url.clone()
                }
            } else { url.clone() }
        } else { url.clone() }
    };

    let context = serde_json::json!({
        "current_page": "config",
        "server": {
            "host": conf.host,
            "port": conf.port,
            "project_dir": conf.project_dir,
            "timezone": conf.tz.as_deref().unwrap_or("UTC"),
            "log_init": conf.log_init,
            "touch_reload": conf.touch_reload,
        },
        "database": {
            "url": db_url_masked,
            "min_connections": conf.database.min_connections,
            "max_connections": conf.database.max_connections,
            "lazy": conf.database.lazy,
        },
        "auth": {
            "access_ttl": conf.auth.access_ttl,
            "refresh_ttl": conf.auth.refresh_ttl,
            "access_cookie": conf.auth.access_cookie.as_ref().map(|c| serde_json::json!({
                "name": c.name, "path": c.path,
                "http_only": c.http_only, "secure": c.secure, "same_site": c.same_site,
            })),
            "refresh_cookie": conf.auth.refresh_cookie.as_ref().map(|c| serde_json::json!({
                "name": c.name, "path": c.path,
                "http_only": c.http_only, "secure": c.secure, "same_site": c.same_site,
            })),
        },
        "tasks": {
            "poll_interval_ms": conf.tasks.poll_interval_ms,
            "capacity": conf.tasks.capacity,
            "concurrency": conf.tasks.concurrency,
            "batch_size": conf.tasks.batch_size,
        },
        "static_dirs": conf.static_dirs.iter().map(|d| serde_json::json!({
            "path": d.path, "url": d.url,
        })).collect::<Vec<_>>(),
        "media_dir": conf.media_dir,
        "templates_dir": conf.templates_dir,
    });
    render(&site, "admin/conf.html", &context)
}


#[bundles::route(path = "/sysfinfo")]
async fn sysfinfo_view(site: Site) -> Result<Html<String>, AdminError> {
    let context = serde_json::json!({ "current_page": "system" });
    render(&site, "admin/sysfinfo.html", &context)
}


#[bundles::route(path = "/kitchen-sink", method = "GET")]
async fn kitchen_sink(site: Site) -> Result<Html<String>, AdminError> {
    let context = serde_json::json!({
        "current_page": "kitchen-sink",
    });
    render(&site, "admin/kitchen-sink.html", &context)
}

#[bundles::route(path = "/stats", method = "GET")]
async fn stats_view(site: Site) -> Result<Html<String>, AdminError> {
    use crate::admin::stats::LiveStats;
    
    let stats = LiveStats::get().ok();
    
    let context = if let Some(s) = stats {
        serde_json::json!({
            "current_page": "stats",
            "cpu_usage": format!("{:.1}", s.cpu_usage),
            "cpu_count": s.cpu_count,
            "memory_total": LiveStats::format_bytes(s.total_memory_bytes),
            "memory_used": LiveStats::format_bytes(s.used_memory_bytes),
            "memory_available": LiveStats::format_bytes(s.available_memory_bytes),
            "memory_percent": format!("{:.1}", s.memory_usage_percent),
            "swap_total": LiveStats::format_bytes(s.total_swap_bytes),
            "swap_used": LiveStats::format_bytes(s.used_swap_bytes),
            "system_uptime": LiveStats::format_uptime(s.uptime),
            "system_uptime_secs": s.uptime,
            "load_avg_1": s.load_average_1.map(|l| format!("{:.2}", l)),
            "load_avg_5": s.load_average_5.map(|l| format!("{:.2}", l)),
            "load_avg_15": s.load_average_15.map(|l| format!("{:.2}", l)),
            "process_cpu": format!("{:.1}", s.process_cpu_usage),
            "process_memory": LiveStats::format_bytes(s.process_memory_bytes),
            "process_memory_percent": format!("{:.2}", s.process_memory_percent),
            "process_virtual_memory": LiveStats::format_bytes(s.process_virtual_memory_bytes),
            "process_uptime": LiveStats::format_uptime(s.process_uptime),
            "process_pid": s.process_pid,
        })
    } else {
        serde_json::json!({
            "current_page": "stats",
            "error": "Unable to fetch system statistics"
        })
    };
    
    render(&site, "admin/stats.html", &context)
}


pub fn admin_bundle() -> bundles::Bundle {
    bundles::bundle!(
        greeting,
        admin_assets_dir,
        static_file,
        operations,
        login_view,
        conf_view,
        sysfinfo_view,
        kitchen_sink,
        stats_view,
    )
}

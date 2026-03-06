use crate::routes::{Html, IntoResponse, Response};
use crate::templates::TemplateError;
use crate::{Site, bundles, embed, routes};
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
                    .header(axum::http::header::LOCATION, "/admin/login")
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

fn render_error(site: &Site, template_name: &str, error_message: &str) -> Response {
    let context = serde_json::json!({
        "error_message": error_message,
    });
    match site.render_template(template_name, &context) {
        Ok(rendered) => Html(rendered).into_response(),
        Err(_) => Html(format!("<h1>Error</h1><p>{}</p>", error_message)).into_response(),
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
async fn greeting(site: Site) -> Result<Html<String>, AdminError> {
    let context = serde_json::json!({
        "greeting": "Hello, Admin!",
    });
    let content = render(&site, "admin/base.html", &context)?;
    Ok(content)
}

#[bundles::route(path = "/operations", method = "GET")]
async fn operations(site: Site) -> Result<Html<String>, AdminError> {
    use std::collections::BTreeMap;
    
    let mut by_kind: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    
    for op in site.iter_operations() {
        let kind = format!("{:?}", op.kind);
        
        let methods: Vec<&str> = op.http_methods();
        
        let op_json = serde_json::json!({
            "name": op.name,
            "summary": op.summary,
            "description": op.description,
            "path": op.path,
            "methods": methods,
            "args": op.args.iter().map(|arg| {
                let type_name = match &arg.part {
                    crate::callables::ArgPart::Header(ts) |
                    crate::callables::ArgPart::Cookie(ts) |
                    crate::callables::ArgPart::Query(ts) |
                    crate::callables::ArgPart::Path(ts) |
                    crate::callables::ArgPart::Body(ts, _) => (ts.type_name)().to_string(),
                    crate::callables::ArgPart::Security { scheme, .. } => scheme.to_string(),
                    crate::callables::ArgPart::Zone => "Zone".to_string(),
                    crate::callables::ArgPart::Ignore => "Ignored".to_string(),
                };
                serde_json::json!({
                    "name": arg.name,
                    "type_name": type_name,
                    "description": arg.description,
                })
            }).collect::<Vec<_>>(),
            "returns": op.returns.iter().map(|ret| {
                let type_name = match &ret.part {
                    crate::callables::ReturnPart::Header(ts) |
                    crate::callables::ReturnPart::Body(ts, _) => (ts.type_name)().to_string(),
                    crate::callables::ReturnPart::Empty => "Empty".to_string(),
                    crate::callables::ReturnPart::Unknown => "Unknown".to_string(),
                };
                serde_json::json!({
                    "status": ret.status_code,
                    "type_name": type_name,
                    "description": ret.description,
                })
            }).collect::<Vec<_>>(),
            "tags": op.tags,
        });
        
        by_kind.entry(kind).or_default().push(op_json);
    }
    
    let operations_by_kind: Vec<_> = by_kind.into_iter()
        .map(|(kind, ops)| (kind, ops))
        .collect();
    
    let context = serde_json::json!({
        "operations_by_kind": operations_by_kind,
    });
    
    let content = render(&site, "admin/operations.html", &context)?;
    Ok(content)
}


#[bundles::asset_dir]
fn admin_assets_dir() -> embed::Dir {
    embed::Dir::new(ADMIN_ASSETS.clone())
}





#[bundles::route(path = "/login")]
async fn login_view() -> Response {
    let body = "<h1>Admin Login</h1><p>Please log in to access the admin interface.</p>";
    Html(body.to_string()).into_response()
}


#[bundles::route(path = "/conf")]
async fn conf_view() -> Response {
    let body = "<h1>Admin Configuration</h1><p>Configuration settings go here.</p>";
    Html(body.to_string()).into_response()
}   


#[bundles::route(path = "/sysfinfo")]
async fn sysfinfo_view() -> Response {
    let body = "<h1>System Information</h1><p>System info details go here.</p>";
    Html(body.to_string()).into_response()
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
        operations,
        login_view,
        conf_view,
        sysfinfo_view,
        kitchen_sink,
        stats_view,
    )
}

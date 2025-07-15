use axum::{body::Body, extract::Request, http::{StatusCode, Uri}, response::{IntoResponse, Response}};
use std::str::FromStr;
use axum::http::header::ACCEPT;

// Define X_REQUESTED_WITH manually
const X_REQUESTED_WITH: &str = "x-requested-with";


fn is_api_request<B>(req: &Request<B>) -> bool {
    let accept_is_not_html = req
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| !v.contains("text/html"))
        .unwrap_or(true); // assume non-html if missing

    let not_xhr = req
        .headers()
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        .map(|v| v != "XMLHttpRequest")
        .unwrap_or(true); // assume not xhr

    accept_is_not_html && not_xhr
}


pub async fn redirect_trailing_slash_on_404(req: Request<Body>) -> Response {
    let uri = req.uri();
    let path = uri.path();

    // Check if the request is for HTML content
    let is_api_request = is_api_request(&req);

    // Skip if:
    if !is_api_request || path.ends_with('/') || path.contains('.') || path == "/" {
        return (StatusCode::NOT_FOUND, "Not Found").into_response();
    }

    // Build new URI with trailing slash and same query
    let mut new_path = format!("{}/", path);
    if let Some(q) = uri.query() {
        new_path.push('?');
        new_path.push_str(q);
    }

    if let Ok(location) = Uri::from_str(&new_path) {
        return (
            StatusCode::PERMANENT_REDIRECT,
            [(axum::http::header::LOCATION, location.to_string())],
        )
            .into_response();
    }

    (StatusCode::NOT_FOUND, "Invalid redirect").into_response()
}


/// Middleware: Rewrite XHR/JSON/other non-browser paths to include trailing `/`
pub fn rewrite_request_path<B>(mut req: Request<B>) -> Request<B> {
    let uri = req.uri();
    let path = uri.path();

    let is_non_browser = is_api_request(&req);

    if !is_non_browser || path == "/" || path.ends_with('/') || path.rsplit('/').next().map_or(false, |s| s.contains('.')) {
        return req;
    }

    let mut new_path = format!("{}/", path);
    if let Some(q) = uri.query() {
        new_path.push('?');
        new_path.push_str(q);
    }

    if let Ok(pq) = new_path.parse() {
        let mut parts = uri.clone().into_parts();
        parts.path_and_query = Some(pq);
        if let Ok(new_uri) = Uri::from_parts(parts) {
            *req.uri_mut() = new_uri;
        }
    }

    req
}
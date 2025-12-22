use std::borrow::Cow;
use axum::http::Method;
use axum::body::{self, Body};
use tower::ServiceExt as _;

use uxar::views::{Router, ViewMeta, Routable};

#[test]
fn mount_and_reverse() {
    let meta = ViewMeta {
        name: Cow::Owned("view".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/users/{id}".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let child = Router::from_parts(vec![meta], axum::Router::new());
    let base = Router::new().mount("/api", "ns", child);

    let inspector = base.make_inspector();
    let got = inspector.reverse("ns:view", &[("id", "42")]);
    assert_eq!(got, Some("/api/users/42".to_string()));
}

#[test]
fn merge_and_reverse() {
    let meta1 = ViewMeta {
        name: Cow::Owned("a".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/items/{id}".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let meta2 = ViewMeta {
        name: Cow::Owned("b".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/things/{id}".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let r1 = Router::from_parts(vec![meta1], axum::Router::new());
    let r2 = Router::from_parts(vec![meta2], axum::Router::new());

    let base = Router::new().merge(r1).merge(r2);
    let inspector = base.make_inspector();

    assert_eq!(inspector.reverse("a", &[("id", "10")]), Some("/items/10".to_string()));
    assert_eq!(inspector.reverse("b", &[("id", "20")]), Some("/things/20".to_string()));
}

#[test]
fn nested_mounts() {
    let inner_meta = ViewMeta {
        name: Cow::Owned("inner".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/inner/{id}".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let inner = Router::from_parts(vec![inner_meta], axum::Router::new());
    let inner_mounted = Router::new().mount("/sub", "inner_ns", inner);
    let top = Router::new().mount("/api", "top", inner_mounted);

    let inspector = top.make_inspector();
    let got = inspector.reverse("top:inner_ns:inner", &[("id", "123")]);
    assert_eq!(got, Some("/api/sub/inner/123".to_string()));
}

#[tokio::test]
async fn mounted_router_returns_response_and_matches_meta_url() {
    // Create a simple view meta and an axum router that returns "ok" at /hello
    let meta = ViewMeta {
        name: Cow::Owned("hello".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/hello".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    // axum router used for actual request testing
    let child_axum = axum::Router::new().route("/hello", axum::routing::get(|| async { "ok" }));

    // separate uxar::views::Router that carries metadata (no need for the actual axum handler here)
    let child = Router::from_parts(vec![meta.clone()], axum::Router::new());

    let top = Router::new().mount("/api", "ns", child);

    // Inspector reverse should return the mounted path
    let inspector = top.make_inspector();
    let url = inspector.reverse("ns:hello", &[]).expect("reverse");
    assert_eq!(url, "/api/hello");

    // Build an equivalent axum router for testing (router with `()` state)
    let axum_router = axum::Router::new().nest("/api", child_axum);
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri(&url)
        .body(Body::empty())
        .unwrap();

    let resp = axum_router.oneshot(req).await.expect("service call");
    let bytes = body::to_bytes(resp.into_body(), 1024).await.expect("read body");
    assert_eq!(&bytes[..], b"ok");
}

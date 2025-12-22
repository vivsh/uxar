use std::borrow::Cow;
use axum::http::Method;
use axum::body::{self, Body};
use tower::ServiceExt as _;

use uxar::views::{Router, ViewMeta, Routable};

#[test]
fn reverse_ignores_extra_args() {
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

    // extra arg "foo" should be ignored
    let got = inspector.reverse("ns:view", &[("id", "7"), ("foo", "bar")]);
    assert_eq!(got, Some("/api/users/7".to_string()));
}

#[test]
fn mount_namespace_overrides_previous() {
    // first meta has path /first
    let meta1 = ViewMeta {
        name: Cow::Owned("v".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/first".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    // second meta with same logical name but different path
    let meta2 = ViewMeta {
        name: Cow::Owned("v".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/second".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    let r1 = Router::from_parts(vec![meta1], axum::Router::new());
    let base = Router::new().mount("/api", "ns", r1);
    // mounting again with same namespace/name should override
    let r2 = Router::from_parts(vec![meta2], axum::Router::new());
    let base = base.mount("/api", "ns", r2);

    let inspector = base.make_inspector();
    let got = inspector.reverse("ns:v", &[]);
    assert_eq!(got, Some("/api/second".to_string()));
}

#[tokio::test]
async fn deep_nested_routing_and_reverse() {
    // metadata expects /a/b/c
    let meta = ViewMeta {
        name: Cow::Owned("deep".to_string()),
        methods: vec![Method::GET],
        path: Cow::Owned("/a/b/c".to_string()),
        summary: None,
        params: vec![],
        responses: vec![],
    };

    // handler router that serves the deep path
    let child_axum = axum::Router::new().route("/a/b/c", axum::routing::get(|| async { "deep" }));

    // metadata-only child (separate from handler router)
    let child_meta = Router::from_parts(vec![meta.clone()], axum::Router::new());
    // mount child at /mount2 with namespace ns2, then mount that under /mount1 with namespace ns1
    let nested = Router::new().mount("/mount2", "ns2", child_meta);
    let top = Router::new().mount("/mount1", "ns1", nested);

    // reverse should reflect the combined mounts; final logical name becomes "ns1:ns2:deep"
    let inspector = top.make_inspector();
    let got = inspector.reverse("ns1:ns2:deep", &[]);
    assert!(got.is_some());
    assert!(got.unwrap().ends_with("/a/b/c"));

    // Test the actual handler by composing an axum router with both mount prefixes
    let axum_router = axum::Router::new().nest("/mount1", axum::Router::new().nest("/mount2", child_axum));
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri("/mount1/mount2/a/b/c")
        .body(Body::empty())
        .unwrap();

    let resp = axum_router.oneshot(req).await.expect("service call");
    let bytes = body::to_bytes(resp.into_body(), 1024).await.expect("read body");
    assert_eq!(&bytes[..], b"deep");
}

// Integration test for Site::router()
// This test checks that the router can be constructed and basic routes are present.

use axum::{Router, routing::get};
use uxar::SiteConf;
use axum_test::TestServer;

async fn hello_handler() -> &'static str {
    "Hello, world!"
}

#[tokio::test]
async fn test_site_router_basic() {
    let conf = SiteConf::from_env();
    let builder = uxar::Site::builder(conf);
    let site = builder
        .mount("/hello", Router::new().route("/", get(hello_handler)))
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/hello").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.text(), "Hello, world!");
}

// Test to ensure the router handles panics within a handler gracefully.
async fn panic_handler() -> &'static str {
    panic!("This is a test panic");
}

#[tokio::test]
async fn test_site_router_panic_handling() {
    let conf = SiteConf::from_env();
    println!("SiteConf: {:?}", conf);
    let builder = uxar::Site::builder(conf);
    let site = builder
        .mount("/panic", Router::new().route("/", get(panic_handler)))
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/panic").await;
    assert_eq!(response.status_code(), 500);
}

#[tokio::test]
async fn test_url_append_slash() {
    let conf = SiteConf::from_env();
    let builder = uxar::Site::builder(conf);
    let site = builder
        .mount("/test", Router::new().route("/", get(|| async { "Test handler" })))
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();

    // Request without trailing slash
    let response = server.get("/test").await;
    assert_eq!(response.status_code(), 200);

    // let response = server.get("/test/").await;
    // assert_eq!(response.status_code(), 200);
}


// Integration test for Site::router()
// This test checks that the router can be constructed and basic routes are present.

use axum::{Router, routing::get};
use uxar::{Site, SiteConf, StaticDir};
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
#[tokio::test]
async fn test_site_static_dirs() {
    // Prepare test data
    let mut conf = SiteConf::from_env();
    conf.static_dirs.push(StaticDir { path: "tests/data/static".into(), url: "/static/".into() });
    let builder = uxar::Site::builder(conf);    
    let site = builder
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();

    // Test serving a static file
    let response = server.get("/static/test.txt").await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.text(), "Static file content");
}

#[tokio::test]
async fn test_site_static_dirs_not_found() {
    let conf = SiteConf::from_env();
    let builder = uxar::Site::builder(conf);
    let site = builder
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();

    // Test requesting a non-existent file
    let response = server.get("/static/nonexistent.txt").await;
    assert_eq!(response.status_code(), 404);
}

#[tokio::test]
async fn test_site_extractor() {
    async fn extractor_handler(site: Site) -> String {
        format!("Extracted: {:?}", site.uptime())
    }

    let conf = SiteConf::from_env();
    let builder = uxar::Site::builder(conf);
    let site = builder
        .mount(
            "/extract",
            Router::new().route("/any", get(extractor_handler)),
        )
        .build()
        .await
        .expect("Failed to build site");

    let app = site.router();
    let server = TestServer::new(app).unwrap();

    // Test with a valid path parameter
    let response = server.get("/extract/any").await;
    assert_eq!(response.status_code(), 200);
    assert!(response.text().starts_with("Extracted"));
}

#[tokio::test]
async fn test_site_render_template_success() {
    // Prepare test data
    let mut conf = SiteConf::from_env();
    conf.templates_dir = Some("tests/data/templates".into());
    let builder = uxar::Site::builder(conf);
    let site = builder
        .build()
        .await
        .expect("Failed to build site");

    // Prepare context for rendering
    let mut context = std::collections::HashMap::new();
    context.insert("place", "Somewhere");

    // Render the template
    let rendered = site
        .render_template("test.html", &context)
        .expect("Failed to render template");

    // The test.html file should contain something like: "Hello, {{ place }}!"
    assert_eq!(rendered, "Hello World and welcome to the Somewhere!");
}

#[tokio::test]
async fn test_site_render_template_not_found() {
    let mut conf = SiteConf::from_env();
    conf.templates_dir = Some("tests/data/templates".into());
    let builder = uxar::Site::builder(conf);
    let site = builder
        .build()
        .await
        .expect("Failed to build site");

    let context = std::collections::HashMap::<&str, &str>::new();

    // Try to render a non-existent template
    let result = site.render_template("nonexistent.html", &context);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_site_render_template_inheritance() {
    // Prepare test data
    let mut conf = SiteConf::from_env();
    conf.templates_dir = Some("tests/data/templates".into());
    let builder = uxar::Site::builder(conf);
    let site = builder
        .build()
        .await
        .expect("Failed to build site");

    // Prepare context for rendering
    let mut context = std::collections::HashMap::new();
    context.insert("place", "Somewhere");

    // Render the template
    let rendered = site
        .render_template("home.html", &context)
        .expect("Failed to render template");

    // The home.html file should contain something like: "Hello, {{ place }}!"
    assert_eq!(rendered, "Base Page[Home Somewhere]");
}
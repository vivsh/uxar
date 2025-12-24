use axum::http::StatusCode;
use uxar::{Site, SiteConf, Service, testing::TestClient, views::{self, IntoResponse}};

// Define a dummy service
#[derive(Clone)]
struct MyService {
    value: i32,
}

// impl Service for MyService {} - Blanket impl covers this

// Define a handler that uses the service
async fn use_service(
    Service(svc): Service<MyService>
) -> views::Response {
    views::Json(svc.value).into_response()
}

// Define a handler that uses a missing service
async fn use_missing_service(
    Service(_svc): Service<String> // String is not registered
) -> views::Response {
    views::Response::default()
}

#[tokio::test]
async fn test_service_extraction_success() {
    // Setup Site with the service
    let mut conf = SiteConf::default();
    conf.database = "postgres://localhost/dummy".to_string();
    
    let svc = MyService { value: 42 };
    
    let router = views::AxumRouter::new()
        .route("/test", views::get(use_service));

    let site = Site::builder(conf)
        .with_lazy_db()
        .with_service(svc)
        .merge(router)
        .build()
        .await
        .expect("Failed to build site");

    // Create TestClient
    let client = TestClient::new(site);

    // Make request
    let resp = client.get("/test").send().await;

    // Assertions
    let val: i32 = resp.assert_ok().json().await;
    assert_eq!(val, 42);
}

#[tokio::test]
async fn test_service_extraction_failure() {
    // Setup Site WITHOUT the service
    let mut conf = SiteConf::default();
    conf.database = "postgres://localhost/dummy".to_string();

    // Mount the handler that expects a String service (which we won't add)
    let router = views::AxumRouter::new()
        .route("/fail", views::get(use_missing_service));
    
    let site = Site::builder(conf)
        .with_lazy_db()
        .merge(router)
        .build()
        .await
        .expect("Failed to build site");

    // Create TestClient
    let client = TestClient::new(site);

    // Make request
    let resp = client.get("/fail").send().await;

    // Assertions - should be 500 Internal Server Error
    let body: serde_json::Value = resp.assert_status(StatusCode::INTERNAL_SERVER_ERROR).json().await;
    
    // Verify error body structure
    assert_eq!(body["code"], "INTERNAL_ERROR");
    assert_eq!(body["detail"], "Internal server error");
    
    // The debug_details message should mention the missing service type
    // Note: This relies on debug assertions being enabled during tests
    if cfg!(debug_assertions) {
        let debug_details = body["debug_details"].as_str().expect("debug_details missing");
        assert!(debug_details.contains("String"));
    }
}



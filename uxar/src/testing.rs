use crate::{Site, SiteConf};
use crate::db::{DbConf, Pool};
use axum::body::{self, Body, Bytes};
use axum::http::{Request, Method, Response};
use axum::Router;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::ServiceExt;
use std::collections::BTreeMap;
use std::ops::Deref;
use serde_json::{self, value::to_value, Value};

pub use sqlx::{test_block_on, test};

pub struct TestClient {
    app: Router,    
    site: Site,
}

impl TestClient {
    pub fn new(site: Site) -> Self {
        let app = site.clone().router();
        Self { app, site }
    }

    pub fn request(&self, method: Method, path: &str) -> TestRequestBuilder {
        TestRequestBuilder::new(self.app.clone(), method, path)
    }

    pub fn get(&self, path: &str) -> TestRequestBuilder {
        self.request(Method::GET, path)
    }
    pub fn post(&self, path: &str) -> TestRequestBuilder {
        self.request(Method::POST, path)
    }
    pub fn put(&self, path: &str) -> TestRequestBuilder {
        self.request(Method::PUT, path)
    }
    pub fn delete(&self, path: &str) -> TestRequestBuilder {
        self.request(Method::DELETE, path)
    }
    pub fn patch(&self, path: &str) -> TestRequestBuilder {
        self.request(Method::PATCH, path)
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        self.site.shutdown();
    }
}

pub struct TestRequestBuilder {
    app: Router,
    method: Method,
    path: String,
    headers: Vec<(String, String)>,
    body: Option<Body>,
}

impl TestRequestBuilder {
    pub fn new(app: Router, method: Method, path: &str) -> Self {
        Self {
            app,
            method,
            path: path.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    pub fn json<T: Serialize>(mut self, value: &T) -> Self {
        let json = serde_json::to_vec(value).expect("Failed to serialize JSON");
        self.body = Some(Body::from(json));
        self.headers.push(("content-type".to_string(), "application/json".to_string()));
        self
    }

    pub fn query<T: Serialize>(mut self, params: &[(&str, T)]) -> Self {
        let query = TestClient::build_query(params);
        if self.path.contains('?') {
            self.path = format!("{}&{}", self.path, query);
        } else {
            self.path = format!("{}?{}", self.path, query);
        }
        self
    }

    pub async fn send(self) -> TestResponse {
        let mut req = Request::builder()
            .method(self.method)
            .uri(self.path);
        for (k, v) in self.headers {
            req = req.header(&k, &v);
        }
        let req = req.body(self.body.unwrap_or_else(|| Body::empty())).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();        
        TestResponse { resp }
    }
}

#[derive(Debug)]
pub struct TestResponse {
    resp: Response<Body>,
}

impl TestResponse {
    pub fn status(&self) -> axum::http::StatusCode {
        self.resp.status()
    }
    pub async fn text(self) -> String {
        let bytes = body::to_bytes(self.resp.into_body(), usize::MAX).await.expect("Failed to read body");
        String::from_utf8(bytes.to_vec()).expect("Response was not valid UTF-8")
    }
    pub async fn bytes(self) -> Bytes {
        body::to_bytes(self.resp.into_body(), usize::MAX).await.expect("Failed to read body")
    }
    pub async fn json<T: DeserializeOwned>(self) -> T {
        let bytes = body::to_bytes(self.resp.into_body(), usize::MAX).await.expect("Failed to read body");
        serde_json::from_slice(&bytes).expect("Response was not valid JSON")
    }
    pub async fn assert_text(self, expected_status: axum::http::StatusCode, expected_body: &str) {
        assert_eq!(self.status(), expected_status);
        let body = self.text().await;
        assert_eq!(body, expected_body);
    }
    pub async fn assert_json<T: DeserializeOwned + PartialEq + std::fmt::Debug>(self, expected_status: axum::http::StatusCode, expected_json: &T) {
        assert_eq!(self.status(), expected_status);
        let body: T = self.json().await;
        assert_eq!(&body, expected_json);
    }

    pub fn assert_status(self, expected_status: axum::http::StatusCode) -> Self {
        assert_eq!(self.status(), expected_status, "Expected status {}, got {}", expected_status, self.status());
        self
    }

    pub fn assert_ok(self) -> Self {
        self.assert_status(axum::http::StatusCode::OK)
    }

    pub fn assert_created(self) -> Self {
        self.assert_status(axum::http::StatusCode::CREATED)
    }

    pub fn assert_not_found(self) -> Self {
        self.assert_status(axum::http::StatusCode::NOT_FOUND)
    }

    pub fn assert_bad_request(self) -> Self {
        self.assert_status(axum::http::StatusCode::BAD_REQUEST)
    }
}

impl TestClient {
    pub fn build_query<T: Serialize>(params: &[(&str, T)]) -> String {
        let mut map = BTreeMap::new();
        for (k, v) in params {
            let value: Value = to_value(v).expect("Failed to serialize param");
            let s = match value {
                Value::String(s) => s,
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => value.to_string(),
            };
            map.insert(*k, s);
        }
        serde_urlencoded::to_string(&map).unwrap()
    }
}

/// Creates a minimal mock Site for testing purposes
/// Uses lazy DB (no actual connection) and safe defaults
pub async fn mock_site() -> SiteConf {
    use uuid::Uuid;
    
    let test_db_name = format!("uxar_test_{}", Uuid::now_v7().simple());
    let conf = SiteConf {
        host: "localhost".to_string(),
        port: 8080,
        project_dir: "/tmp/uxar_test".to_string(),
        database: DbConf::default(),
        secret_key: "test_secret_key_minimum_32_chars!".to_string(),
        static_dirs: vec![],
        media_dir: None,
        templates_dir: None,
        touch_reload: None,
        log_init: false,
        tz: Some("UTC".to_string()),
        auth: crate::auth::AuthConf::default(),
        ..Default::default()
    };

    conf
}

/// RAII guard for a test database
/// Automatically drops the database when the guard is dropped
pub struct MockDb {
    pool: Pool,
    pub db_name: String,
    pub base_url: String,
}

impl MockDb {
    pub fn pool(&self) -> &Pool {
        &self.pool
    }
}

impl Deref for MockDb {
    type Target = Pool;
    
    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl Drop for MockDb {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let base_url = self.base_url.clone();
        
        #[cfg(feature = "postgres")]
        {
            if !db_name.is_empty() {
                let _ = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().ok()?;
                    rt.block_on(async {
                        if let Ok(root_pool) = sqlx::PgPool::connect(&base_url).await {
                            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", db_name))
                                .execute(&root_pool)
                                .await;
                            root_pool.close().await;
                        }
                        Some(())
                    })
                }).join();
            }
        }
        
        #[cfg(feature = "mysql")]
        {
            if !db_name.is_empty() {
                let _ = std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().ok()?;
                    rt.block_on(async {
                        if let Ok(root_pool) = sqlx::MySqlPool::connect(&base_url).await {
                            let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS `{}`", db_name))
                                .execute(&root_pool)
                                .await;
                            root_pool.close().await;
                        }
                        Some(())
                    })
                }).join();
            }
        }
        
        #[cfg(feature = "sqlite")]
        {
            // SQLite uses :memory:, no cleanup needed
        }
    }
}

/// Creates a new isolated database for testing
/// Similar to sqlx test macros, creates a unique database that is cleaned up after use
/// Returns a MockDb guard that derefs to Pool and drops the database on drop
/// 
/// # Example
/// ```ignore
/// #[tokio::test]
/// async fn test_something() {
///     let db = mock_db().await;
///     // Use db like a Pool - it derefs automatically
///     sqlx::query("SELECT 1").execute(&*db).await.unwrap();
///     // Database is dropped when db goes out of scope
/// }
/// ```
pub async fn mock_db() -> MockDb {
    use uuid::Uuid;
    
    #[cfg(feature = "postgres")]
    {
        let base_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://localhost".to_string());
        
        let db_name = format!("uxar_test_{}", Uuid::now_v7().simple());
        
        let root_pool = sqlx::PgPool::connect(&base_url)
            .await
            .expect("Failed to connect to postgres");
        
        sqlx::query(&format!("CREATE DATABASE \"{}\"", db_name))
            .execute(&root_pool)
            .await
            .expect("Failed to create test database");
        
        root_pool.close().await;
        
        let test_url = if base_url.contains('/') {
            let parts: Vec<&str> = base_url.rsplitn(2, '/').collect();
            format!("{}/{}", parts[1], db_name)
        } else {
            format!("{}/{}", base_url, db_name)
        };
        
        let pool = sqlx::PgPool::connect(&test_url)
            .await
            .expect("Failed to connect to test database");
        
        MockDb {
            pool,
            db_name,
            base_url,
        }
    }
    
    #[cfg(feature = "mysql")]
    {
        let base_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "mysql://localhost".to_string());
        
        let db_name = format!("uxar_test_{}", Uuid::now_v7().simple());
        
        let root_pool = sqlx::MySqlPool::connect(&base_url)
            .await
            .expect("Failed to connect to mysql");
        
        sqlx::query(&format!("CREATE DATABASE `{}`", db_name))
            .execute(&root_pool)
            .await
            .expect("Failed to create test database");
        
        root_pool.close().await;
        
        let test_url = if base_url.contains('/') {
            let parts: Vec<&str> = base_url.rsplitn(2, '/').collect();
            format!("{}/{}", parts[1], db_name)
        } else {
            format!("{}/{}", base_url, db_name)
        };
        
        let pool = sqlx::MySqlPool::connect(&test_url)
            .await
            .expect("Failed to connect to test database");
        
        MockDb {
            pool,
            db_name,
            base_url,
        }
    }
    
    #[cfg(feature = "sqlite")]
    {
        let pool = sqlx::SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory sqlite database");
        
        MockDb {
            pool,
            db_name: String::new(),
            base_url: String::new(),
        }
    }
}
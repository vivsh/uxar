use crate::{Site, SiteConf};
use axum::body::{self, Body, Bytes};
use axum::http::{Request, Method, Response};
use axum::Router;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::ServiceExt;
use std::collections::BTreeMap;
use serde_json::{self, value::to_value, Value};

pub struct TestClient {
    app: Router,    
}

impl TestClient {
    pub fn new(site: Site) -> Self {
        let app = site.router();
        Self { app }
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


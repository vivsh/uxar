use axum::{
    Router,
    body::Body,
    http::{Request, Response},
};
use std::{
    collections::HashMap,
    convert::Infallible,
    task::{Context, Poll},
};
use tower::Service;
use tower::{ServiceExt as _, util::BoxService};

pub struct HostService {
    routes: HashMap<String, BoxService<Request<Body>, Response<Body>, Infallible>>,
    not_found: BoxService<Request<Body>, Response<Body>, Infallible>,
}

impl HostService {
    pub fn new() -> Self {
        let not_found = Router::new()
            .fallback(|| async {
                Response::builder()
                    .status(404)
                    .body(Body::from("Not found"))
                    .unwrap()
            })
            .into_service()
            .boxed();
        Self {
            routes: HashMap::new(),
            not_found,
        }
    }

    pub fn with_default(mut self, router: Router) -> Self {
        self.not_found = router.into_service().boxed();
        self
    }

    pub fn has_host(&self, host: &str) -> bool {
        self.routes.contains_key(host)
    }

    pub fn add(mut self, hostname: &str, router: Router) -> Self {
        self.routes
            .insert(hostname.into(), router.into_service().boxed());
        self
    }
}

impl Service<Request<Body>> for HostService {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future =
        <BoxService<Request<Body>, Response<Body>, Infallible> as Service<Request<Body>>>::Future;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(())) // Always ready
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let hostname = req
            .headers()
            .get("host")
            .and_then(|v| v.to_str().ok())
            .and_then(|host| host.split(':').next()) // Get the hostname without port
            .unwrap_or("")
            .to_string();

        let svc = self
            .routes
            .get_mut(&hostname)
            .unwrap_or(&mut self.not_found);
        svc.call(req)
    }
}

impl std::fmt::Debug for HostService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostService")
            .field("routes", &self.routes.keys().collect::<Vec<_>>())
            .finish()
    }
}

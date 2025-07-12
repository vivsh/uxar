use std::{convert::Infallible, sync::Arc};

use async_trait::async_trait;
use axum::{extract::Request, http::{request::Parts, StatusCode}, response::IntoResponse};
use axum::response::Response;
use tower::Service;

use crate::Site;

#[derive(Clone)]
pub struct DeferredWrapper(pub Arc<dyn DeferredResponse + Send + Sync>);

#[async_trait]
pub trait DeferredResponse {
    async fn into_response(&self, parts: &Parts, site: &Site) -> Response;
}

impl IntoResponse for DeferredWrapper {
    fn into_response(self) -> Response {
        let mut resp = StatusCode::NOT_IMPLEMENTED.into_response();
        resp.extensions_mut().insert(self.0);
        resp
    }
}

pub struct DeferredService;

impl Service<Request> for DeferredService {
    type Response = Response;
    type Error = Infallible;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let site = req.extensions().get::<Arc<Site>>().cloned();
        let def = req.extensions().get::<DeferredWrapper>().cloned();
        let (parts, _body) = req.into_parts();

        Box::pin(async move {
            if let (Some(def), Some(site)) = (def, site) {
                return Ok(def.0.into_response(&parts, &site).await);
            }
            Ok(StatusCode::NOT_IMPLEMENTED.into_response())
        })
    }
}
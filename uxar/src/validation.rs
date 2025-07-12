// use std::ops::{Deref, DerefMut};
// use axum::{extract::{FromRequest}, http::{Request, StatusCode}, response::IntoResponse};
// use axum::body::{Body, Bytes};
// use axum::Json;
// use schemars::JsonSchema;
// use serde::{de::DeserializeOwned};
// use serde_json::Value;
// use garde::Validate;
// use async_trait::async_trait;


// #[derive(Debug, Clone)]
// pub struct Valid<T>(pub T);

// impl<T> Deref for Valid<T> {
//     type Target = T;
//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

// impl<T> DerefMut for Valid<T> {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.0
//     }
// }

// #[async_trait]
// impl<S, T> FromRequest<S> for Valid<Json<T>>
// where
//     S: Send + Sync,
//     T: DeserializeOwned + Validate + JsonSchema + Send,
// {
//     type Rejection = axum::response::Response;

//     async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
//         let (parts, body) = req.into_parts();
//         let content_type = parts.headers.get(axum::http::header::CONTENT_TYPE)
//             .and_then(|v| v.to_str().ok()).unwrap_or("");

//         let body_bytes = hyper::body::to_bytes(body).await.map_err(|e| {
//             (StatusCode::BAD_REQUEST, format!("Invalid body: {e}")).into_response()
//         })?;

//         let raw: Value = serde_json::from_slice(&body_bytes).map_err(|e| {
//             (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response()
//         })?;

//         if content_type.starts_with("application/json") {
//             let schema = schemars::schema_for!(T);
//             let compiled = JsonSchema::compile(&schema.schema).map_err(|e| {
//                 (StatusCode::INTERNAL_SERVER_ERROR, format!("Schema error: {e}")).into_response()
//             })?;
//             if let Err(errors) = compiled.validate(&raw) {
//                 let mut map = serde_json::Map::new();
//                 for err in errors {
//                     let path = err.instance_path.to_string();
//                     map.entry(path).or_insert_with(|| Value::Array(vec![]))
//                         .as_array_mut().unwrap()
//                         .push(Value::String(err.to_string()));
//                 }
//                 return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(map)).into_response());
//             }
//         }

//         let inner: T = serde_json::from_value(raw).map_err(|e| {
//             (StatusCode::BAD_REQUEST, format!("Deserialization error: {e}")).into_response()
//         })?;

//         if let Err(report) = inner.validate() {
//             let mut errors = serde_json::Map::new();
//             for (path, err) in report.iter() {
//                 errors.entry(path.to_string())
//                     .or_insert_with(|| Value::Array(vec![]))
//                     .as_array_mut().unwrap()
//                     .push(Value::String(err.to_string()));
//             }
//             return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(errors)).into_response());
//         }

//         Ok(Valid(Json(inner)))
//     }
// }




// #[async_trait]
// impl<S, T> FromRequest<S> for Valid<T>
// where
//     S: Send + Sync,
//     T: FromRequest<S> + Validate,
//     T::Body: Send,
// {
//     type Rejection = axum::response::Response;

//     async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
//         let inner = T::from_request(req, state).await.map_err(IntoResponse::into_response)?;
//         if let Err(report) = inner.validate() {
//             let mut errors = serde_json::Map::new();
//             for (path, err) in report.iter() {
//                 errors.entry(path.to_string())
//                     .or_insert_with(|| Value::Array(vec![]))
//                     .as_array_mut().unwrap()
//                     .push(Value::String(err.to_string()));
//             }
//             return Err((StatusCode::UNPROCESSABLE_ENTITY, Json(errors)).into_response());
//         }
//         Ok(Valid(inner))
//     }
// }
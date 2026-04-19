//! Extractors for extracting data from context in callable handlers.

use super::specs::{
    ArgPart, CallError, IntoArgPart,
    IntoReturnPart, Payloadable, ReturnPart, TypeSchema,
};
use super::callables::{
    FromContext, FromContextParts, IntoOutput, IntoPayloadData, PayloadData,
};
use crate::routes::JsonStr;
use crate::{AuthUser, Site, site};
use schemars::JsonSchema;
use std::borrow::Cow;
use std::sync::Arc;


/// Trait for context types that provide access to a `Site` instance.
pub trait HasSite {
    fn site(&self) -> &Site;
}

/// Trait for types that can be extracted from a `Site`.
pub trait FromSite: Sized + Send {
    fn from_site(site: &Site) -> Result<Self, CallError>;
}

/// Generic implementation: any `T: FromSite` can be extracted from any `C: HasSite`.
impl<C: HasSite, T: FromSite> FromContextParts<C> for T {
    fn from_context_parts(ctx: &C) -> Result<Self, CallError> {
        T::from_site(ctx.site())
    }
}

/// Wrapper for typed payloads extracted from context.
/// Uses `Arc` internally for cheap cloning when broadcasting to multiple handlers.
/// Implements `Deref` for transparent access to inner value.
pub struct Payload<T: Payloadable> {
    inner: Arc<T>,
}


impl<T: Payloadable> Payload<T> {
    pub(crate) fn new(inner: T) -> Payload<T> {
        Payload { inner: Arc::new(inner) }
    }
}

impl<T: Payloadable> std::ops::Deref for Payload<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Payloadable> From<Arc<T>> for Payload<T> {
    fn from(value: Arc<T>) -> Self {
        Payload { inner: value }
    }
}

impl<T: Payloadable> From<T> for Payload<T> {
    fn from(value: T) -> Self {
        Payload::new(value)
    }
}

/// `Payload<T>` implements `FromContext` directly (not `FromContextParts`).
/// This ensures it can only appear as the last argument in a handler signature,
/// since `impl_context!` requires all but the last arg to impl `FromContextParts`.
impl<C, T> FromContext<C> for Payload<T>
where
    C: IntoPayloadData + Send + 'static,
    T: Payloadable,
{
    fn from_context(ctx: C) -> Result<Self, CallError> {
        // Extract type-erased payload from context and downcast to T
        let payload_data = ctx.into_payload_data();
        let value = payload_data
            .downcast_arc::<T>()
            .ok_or_else(|| CallError::TypeMismatch)?;
        Ok(Payload::from(value))
    }

    /// Returns a deserializer function for JSON payloads.
    /// This is used by task queues to deserialize string payloads into typed values.
    fn deserializer() -> Option<fn(&str) -> Result<PayloadData, CallError>> {
        Some(|s: &str| {
            let value: T = serde_json::from_str(s).map_err(|_| CallError::DeserializeFailed)?;
            Ok(PayloadData::new(value))
        })
    }
}

impl<T: Payloadable> IntoArgPart for Payload<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Body(TypeSchema::wrap::<T>(), "application/json".into())
    }
}

impl<T: Payloadable> IntoReturnPart for Payload<T> {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Body(TypeSchema::wrap::<T>(), "application/json".into())
    }
}

/// Payload<T> can be returned from handlers as an infallible output.
impl<T: Payloadable, E> IntoOutput<E> for Payload<T> {
    fn into_output(self) -> Result<PayloadData, E> {
        // Already have Box<T>, just wrap in PayloadData
        Ok(PayloadData::from_arc(self.inner))
    }
}

// Common axum extractor implementations

impl<T: JsonSchema + Send + 'static> IntoArgPart for axum::extract::Path<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Path(TypeSchema::wrap::<T>())
    }
}

impl<T: JsonSchema + Send + 'static> IntoArgPart for axum::extract::Query<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Query(TypeSchema::wrap::<T>())
    }
}

impl<T: JsonSchema + Send + 'static> IntoArgPart for axum_extra::extract::Query<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Query(TypeSchema::wrap::<T>())
    }
}

impl<T: JsonSchema + Send + 'static> IntoArgPart for axum::extract::Json<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Body(TypeSchema::wrap::<T>(), Cow::Borrowed("application/json"))
    }
}


impl<T: Clone + Send + Sync + 'static> IntoArgPart for axum::extract::State<T> {
    fn into_arg_part() -> ArgPart {
        ArgPart::Zone // State is not documented in OpenAPI
    }
}


impl IntoReturnPart for axum::response::Response{
    fn into_return_part() -> ReturnPart {
        ReturnPart::Unknown
    }
}



impl<T: JsonSchema + Send + 'static> IntoReturnPart for axum::extract::Json<T> {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Body(TypeSchema::wrap::<T>(), Cow::Borrowed("application/json"))
    }
}

impl IntoReturnPart for axum::http::StatusCode {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Empty
    }
}

impl<T: IntoReturnPart> IntoReturnPart for (axum::http::StatusCode, T) {
    fn into_return_part() -> ReturnPart {
        T::into_return_part()
    }
}

impl IntoReturnPart for axum::response::Html<String> {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Body(
            TypeSchema::wrap::<String>(),
            "text/html".into()
        )
    }
}

impl IntoReturnPart for () {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Empty
    }
}

impl<T, E> IntoReturnPart for Result<T, E> 
where
    T: IntoReturnPart,
    E: Send,
{
    fn into_return_part() -> ReturnPart {
        T::into_return_part()
    }
}

impl <T> IntoReturnPart for Option<T> 
where
    T: IntoReturnPart,
{
    fn into_return_part() -> ReturnPart {
        T::into_return_part()
    }
}

impl<'a> IntoReturnPart for JsonStr{
    fn into_return_part() -> ReturnPart {
        ReturnPart::Body(
            TypeSchema::wrap::<String>(),
            "application/json".into()
        )
    }
}

impl IntoArgPart for AuthUser {
    fn into_arg_part() -> ArgPart {
        ArgPart::Security { 
            scheme: "bearerAuth".into(), 
            join_all: true,
            scopes: Vec::new(),
        }
    }
}

impl IntoArgPart for site::Site {

    fn into_arg_part() -> ArgPart {
        ArgPart::Ignore
    }

}
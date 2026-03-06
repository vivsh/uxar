use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::any::TypeId;
use std::borrow::Cow;
use std::future::Future;

// Custom tuple types to represent handler arguments internally.
// These prevent users from accidentally using std tuples as argument types.
pub struct Tuple1<T1>(pub(crate) T1);
pub struct Tuple2<T1, T2>(pub(crate) T1, pub(crate) T2);
pub struct Tuple3<T1, T2, T3>(pub(crate) T1, pub(crate) T2, pub(crate) T3);
pub struct Tuple4<T1, T2, T3, T4>(pub(crate) T1, pub(crate) T2, pub(crate) T3, pub(crate) T4);
pub struct Tuple5<T1, T2, T3, T4, T5>(pub(crate) T1, pub(crate) T2, pub(crate) T3, pub(crate) T4, pub(crate) T5);
pub struct Tuple6<T1, T2, T3, T4, T5, T6>(pub(crate) T1, pub(crate) T2, pub(crate) T3, pub(crate) T4, pub(crate) T5, pub(crate) T6);

/// Handler payload type constraint: serde + JSON schema + thread-safe.
/// 
/// Automatically implemented for all types satisfying the bounds.
/// Payloadable ensures that Arc wrapped value is always Send
pub trait Payloadable : JsonSchema + Serialize + DeserializeOwned + Send + Sync + 'static{

}

impl <T> Payloadable for T where T: JsonSchema + Serialize + DeserializeOwned + Send + Sync + 'static {}
/// Errors that can occur during handler execution, extraction, and deserialization.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum CallError {
    /// Failed to deserialize payload from JSON
    #[error("Failed to deserialize payload")]
    DeserializeFailed,

    #[error("Failed to serialize payload")]
    SerializeFailed,
    
    /// Payload type mismatch during extraction
    #[error("Payload type mismatch")]
    TypeMismatch,
    
    /// Context extraction failed
    #[error("Extraction failed: {0}")]
    ExtractionFailed(Cow<'static, str>),
    
    /// Required field is missing
    #[error("Missing required field: {0}")]
    MissingField(Cow<'static, str>),
    
    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(Cow<'static, str>),
    
    /// Unauthorized access
    #[error("Unauthorized")]
    Unauthorized,
    
    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(Cow<'static, str>),
    
    /// Catch-all for any other error type
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<std::convert::Infallible> for CallError {
    fn from(e: std::convert::Infallible) -> Self {
        match e {}
    }
}


/// Compile-time type metadata for JSON schema generation and runtime identity.
#[derive(Clone)]
pub struct TypeSchema {
    pub(crate) type_schema: fn(&mut schemars::SchemaGenerator) -> schemars::Schema,

    pub(crate) type_id: fn() -> TypeId,

    pub(crate) type_name: fn() -> &'static str,
}

impl std::fmt::Debug for TypeSchema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct((self.type_name)()).finish()
    }
}

impl serde::Serialize for TypeSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use schemars::generate::SchemaSettings;
        let settings = SchemaSettings::draft07();
        let mut generator = schemars::SchemaGenerator::new(settings);
        let schema = (self.type_schema)(&mut generator);
        schema.serialize(serializer)
    }
}

impl TypeSchema {
    /// Captures compile-time type metadata for `T`.
    pub fn wrap<T: JsonSchema + 'static>() -> Self {
        fn converter<T: JsonSchema>(genr: &mut schemars::SchemaGenerator) -> schemars::Schema {
            genr.subschema_for::<T>()
        }
        Self {
            type_schema: converter::<T>,
            type_id: || TypeId::of::<T>(),
            type_name: || std::any::type_name::<T>(),
        }
    }

    /// Generates JSON schema using provided generator.
    pub fn schema(&self, genr: &mut schemars::SchemaGenerator) -> schemars::Schema {
        (self.type_schema)(genr)
    }

    /// Returns runtime `TypeId` for type checking.
    pub fn type_id(&self) -> TypeId {
        (self.type_id)()
    }
}



/// Describes how a handler argument is extracted from requests.
#[derive(Debug, Clone, Serialize)]
pub enum ArgPart {
    Ignore,
    /// Extracted from HTTP headers
    Header(TypeSchema),

    /// Extracted from HTTP cookies
    Cookie(TypeSchema),

    /// Extracted from query string parameters
    Query(TypeSchema),

    /// Extracted from URL path parameters
    Path(TypeSchema),

    /// Extracted from request body with specified content type
    Body(TypeSchema, Cow<'static, str>),

    /// Security credentials (API key, OAuth token, etc.)
    Security {
        scheme: Cow<'static, str>,
        scopes: Vec<Cow<'static, str>>,
        join_all: bool,
    },

    /// Multi-tenancy zone identifier
    Zone,
}

/// Describes how a handler return value is serialized into responses.
#[derive(Debug, Clone, Serialize)]
pub enum ReturnPart {
    /// Written to HTTP response headers
    Header(TypeSchema),

    /// Serialized to response body with specified content type
    Body(TypeSchema, Cow<'static, str>),

    /// No content (e.g., 204 No Content)
    Empty,

    Unknown
}

/// Provides compile-time extraction metadata for handler arguments.
pub trait IntoArgPart {
    fn into_arg_part() -> ArgPart;
}

/// Provides compile-time extraction metadata for middleware layer or decorator arguments.
pub trait IntoLayerParts {
    fn into_layer_parts() -> Vec<ArgPart>;
}

/// Provides compile-time serialization metadata for handler returns.
pub trait IntoReturnPart: Send {
    fn into_return_part() -> ReturnPart;
}

/// Marker trait indicating handler arguments contain `Payload<T>`.
pub trait HasPayload<T: Payloadable> {}

/// Provides argument specifications for handler signature introspection.
pub trait IntoArgSpecs {
    fn into_arg_specs() -> Vec<ArgSpec>;
}

/// Async handler with typed arguments and output.
/// 
/// Automatically implemented for functions and closures via macro.
pub trait Specable<Args: IntoArgSpecs>: Send {
    type Output: IntoReturnPart;
    type Future: Future<Output = Self::Output> + Send;

    fn call(&self, args: Args) -> Self::Future;
}

/// Provides enhanced handler specification with real parameter names.
/// 
/// Primarily for proc macros to override auto-generated specs with names
/// and descriptions extracted from source code.
pub trait IntoHandlerSpec {
    fn into_spec() -> CallSpec;
}

impl IntoArgSpecs for () {
    fn into_arg_specs() -> Vec<ArgSpec> {
        vec![]
    }
}

/// Implementations of IntoArgSpecs for tuple types
macro_rules! impl_argspec {
    (
        [$($ty:ident),*], $last:ident, $tuple:ident
    ) => {
        impl<$($ty: IntoArgPart,)* $last: IntoArgPart> IntoArgSpecs for $tuple<$($ty,)* $last> {
            fn into_arg_specs() -> Vec<ArgSpec> {
                let mut args = Vec::new();
                #[allow(unused_mut)]
                let mut position = 0;
                $(
                    args.push(ArgSpec {
                        name: format!("arg{}", position),
                        description: None,
                        position,
                        part: $ty::into_arg_part(),
                    });
                    position += 1;
                )*
                args.push(ArgSpec {
                    name: format!("arg{}", position),
                    description: None,
                    position,
                    part: $last::into_arg_part(),
                });
                args
            }

        }
    };
}

impl_argspec!([], T1, Tuple1);
impl_argspec!([T1], T2, Tuple2);
impl_argspec!([T1, T2], T3, Tuple3);
impl_argspec!([T1, T2, T3], T4, Tuple4);
impl_argspec!([T1, T2, T3, T4], T5, Tuple5);
impl_argspec!([T1, T2, T3, T4, T5], T6, Tuple6);


/// Middleware layer argument metadata.
#[derive(Debug, Clone, Serialize)]
pub struct LayerSpec {
    /// Argument name from function signature.
    pub name: String,

    /// Optional documentation string.
    pub description: Option<String>,

    /// Extraction specification.
    pub parts: Vec<ArgPart>,
}

impl LayerSpec {
    /// Creates argument spec with type, position, name, and documentation.
    pub fn from_type<T: IntoLayerParts>(name: &str, doc: &str) -> Self {
        Self {
            name: name.to_string(),
            description: Some(doc.to_string()),
            parts: T::into_layer_parts(),
        }
    }
}


/// Single handler argument metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ArgSpec {
    /// Argument name from function signature.
    pub name: String,

    /// Optional documentation string.
    pub description: Option<String>,

    /// Position in function signature (0-based).
    pub position: usize,

    /// Extraction specification.
    pub part: ArgPart,
}

impl ArgSpec {
    /// Creates argument spec with type, position, name, and documentation.
    pub fn from_type<T: IntoArgPart>(position: usize, name: &str, doc: &str) -> Self {
        Self {
            name: name.to_string(),
            description: Some(doc.to_string()),
            position,
            part: T::into_arg_part(),
        }
    }
}

/// Handler return value metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ReturnSpec {
    /// Optional documentation string.
    pub description: Option<String>,
    /// HTTP status code.
    pub status_code: Option<u16>,
    /// Response specification.
    pub part: ReturnPart,
}

impl ReturnSpec {
    /// Creates return spec with type, optional documentation, and status code.
    pub fn from_type<T: IntoReturnPart>(doc: Option<String>, status_code: Option<u16>) -> Self {
        Self {
            description: doc,
            status_code,
            part: T::into_return_part(),
        }
    }
}

/// Method receiver kind: `self`, `&self`, `&mut self`, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverSpec {
    Ref,
    MutRef,
    Value,
    Box,
    Arc,
    Unknown(&'static str), // To be filled by macros for any other receiver types
}

/// Complete handler signature specification.
/// 
/// Includes arguments, returns, and metadata for API documentation.
#[derive(Debug, Default, Clone)]
pub struct CallSpec {
    pub description: Option<String>,
    pub name: String,
    pub is_method: bool,
    pub receiver: Option<ReceiverSpec>,
    pub args: Vec<ArgSpec>,
    pub returns: Vec<ReturnSpec>,
}

impl CallSpec {
    /// Extracts compile-time specification from handler.
    pub fn new<Args, H>(handler: &H) -> Self
    where
        H: Specable<Args>,
        Args: IntoArgSpecs,
    {
        let _ = handler; // Use the handler to infer types but don't actually need it
        Self {
            description: None,
            name: std::any::type_name::<H>().to_string(),
            is_method: false,
            receiver: None,
            args: Args::into_arg_specs(),
            returns: vec![ReturnSpec {
                description: None,
                status_code: None,
                part: H::Output::into_return_part(),
            }],
        }
    }

    /// Returns number of handler arguments.
    pub fn arity(&self) -> usize {
        self.args.len()
    }

    /// Returns `TypeId` of body argument, if present.
    pub fn payload_type(&self) -> Option<TypeId> {
        self.args.iter().rev().find_map(|arg| {
            if let ArgPart::Body(t, ..) = &arg.part {
                Some(t.type_id())
            } else {
                None
            }
        })
    }

    /// Updates argument at position. Used by proc macros.
    pub(crate) fn set_arg<T: IntoArgPart>(&mut self, position: usize, name: &str, doc: &str) {
        self.args.retain(|a| a.position != position);
        self.args.push(ArgSpec {
            name: name.to_string(),
            description: Some(doc.to_string()),
            position,
            part: T::into_arg_part(),
        });
        self.args.sort_by_key(|a| a.position);
    }

    /// Replaces all return specs. Used by proc macros.
    pub(crate) fn set_returns(&mut self, output: Vec<ReturnSpec>) {
        self.returns = output;
    }

    /// Appends additional return spec. Used by proc macros.
    pub(crate) fn append_return(&mut self, ret: ReturnSpec) {
        self.returns.push(ret);
    }
}

macro_rules! impl_handler {
    (
        [$($ty:ident),*], $last:ident, $tuple:ident
    ) => {
        #[allow(non_snake_case, unused_mut, unused_variables)]
        impl<F, Fut, R, $($ty,)* $last> Specable<$tuple<$($ty,)* $last>> for F
        where
            F: Fn($($ty,)* $last) -> Fut + Send + Sync,
            Fut: Future<Output = R> + Send,
            R: IntoReturnPart,
            $($ty: IntoArgPart,)*
            $last: IntoArgPart,
        {
            type Output = R;
            type Future = Fut;

            fn call(&self, $tuple($($ty,)* $last): $tuple<$($ty,)* $last>) -> Self::Future {
                (self)($($ty,)* $last)
            }
        }
    };
}

impl<F, Fut, R: IntoReturnPart> Specable<()> for F
where
    F: Fn() -> Fut + Send + Sync,
    Fut: Future<Output = R> + Send,
{
    type Output = R;
    type Future = Fut;

    fn call(&self, _args: ()) -> Self::Future {
        (self)()
    }
}

impl_handler!([], T1, Tuple1);
impl_handler!([T1], T2, Tuple2);
impl_handler!([T1, T2], T3, Tuple3);
impl_handler!([T1, T2, T3], T4, Tuple4);
impl_handler!([T1, T2, T3, T4], T5, Tuple5);
impl_handler!([T1, T2, T3, T4, T5], T6, Tuple6);

// HasPayload implementations for custom tuples containing Payload<T>
impl<T: crate::callables::Payloadable> HasPayload<T> for Tuple1<crate::callables::Payload<T>> {}
impl<T: crate::callables::Payloadable, A1> HasPayload<T> for Tuple2<A1, crate::callables::Payload<T>> {}
impl<T: crate::callables::Payloadable, A1, A2> HasPayload<T> for Tuple3<A1, A2, crate::callables::Payload<T>> {}
impl<T: crate::callables::Payloadable, A1, A2, A3> HasPayload<T> for Tuple4<A1, A2, A3, crate::callables::Payload<T>> {}
impl<T: crate::callables::Payloadable, A1, A2, A3, A4> HasPayload<T> for Tuple5<A1, A2, A3, A4, crate::callables::Payload<T>> {}
impl<T: crate::callables::Payloadable, A1, A2, A3, A4, A5> HasPayload<T> for Tuple6<A1, A2, A3, A4, A5, crate::callables::Payload<T>> {}


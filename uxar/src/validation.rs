use std::{borrow::Cow, collections::BTreeMap, ops::Deref, fmt};

use axum::{extract::{FromRequest, FromRequestParts, Request}, http::{HeaderMap, StatusCode, header, request::Parts}, response::{Html, IntoResponse, Response}};

pub use uxar_macros::Validate;

pub use super::validators::*;

/// Represents a single validation failure with error code, message, and optional params.
///
/// # Example
/// ```ignore
/// let err = ValidationError::new("min_length", "Must be at least 3 characters");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub code: Cow<'static, str>,
    pub message: Cow<'static, str>,
    pub params: Vec<(Cow<'static, str>, String)>,
}
impl ValidationError {
    pub fn new(code: impl Into<Cow<'static, str>>, msg: impl Into<Cow<'static, str>>) -> Self {
        Self { code: code.into(), message: msg.into(), params: Vec::new() }
    }

    pub fn custom(msg: impl Into<Cow<'static, str>>) -> Self {
        Self::new("custom", msg)
    }
}

/// A segment in a validation path, representing field access, array indexing, or map key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSeg {
    Field(Cow<'static, str>),
    Index(usize),
    Key(String),
}

/// A validation path tracking the location of an error in nested structures.
///
/// Paths are built from segments (field names, array indices, map keys) and allow
/// precise error reporting for complex nested data.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Path {
    segs: Vec<PathSeg>,
}
impl Path {
    /// Creates a root path (empty path).
    pub fn root() -> Self { Self { segs: Vec::new() } }
    
    /// Returns true if this is the root path.
    pub fn is_root(&self) -> bool { self.segs.is_empty() }
    
    /// Returns the path segments.
    pub fn segments(&self) -> &[PathSeg] { &self.segs }

    pub fn to_string(&self) -> String {
        self.segs.iter().map(|s| match s {
            PathSeg::Field(f) => f.to_string(),
            PathSeg::Index(i) => i.to_string(),
            PathSeg::Key(k) => k.clone(),
        }).collect::<Vec<_>>().join(".")
    }

    /// Returns a new path with the given segment prepended.
    pub fn prefixed(self, prefix: PathSeg) -> Self {
        let mut segs = Vec::with_capacity(self.segs.len() + 1);
        segs.push(prefix);
        segs.extend(self.segs);
        Self { segs }
    }

    pub fn at_field(mut self, name: impl Into<Cow<'static, str>>) -> Self {
        self.segs.push(PathSeg::Field(name.into()));
        self
    }

    pub fn at_index(mut self, idx: usize) -> Self {
        self.segs.push(PathSeg::Index(idx));
        self
    }

    pub fn at_key(mut self, key: impl Into<String>) -> Self {
        self.segs.push(PathSeg::Key(key.into()));
        self
    }
}

/// A validation error at a specific path in the data structure.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub path: Path,
    pub invalid: ValidationError,
}

/// Collection of validation issues, typically returned from `Validate::validate()`.
///
/// Can be converted to flat or nested JSON structures for API responses.
///
/// # Example
/// ```ignore
/// let mut report = ValidationReport::empty();
/// report.push_root(ValidationError::new("invalid", "Something is wrong"));
/// let json = report.to_nested_map();
/// ```
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
}

impl ValidationReport {
    /// Creates an empty report.
    pub fn empty() -> Self { Self { issues: Vec::new() } }
    
    /// Returns true if there are no validation errors.
    pub fn is_empty(&self) -> bool { self.issues.is_empty() }

    pub fn push(&mut self, path: Path, invalid: ValidationError) {
        self.issues.push(ValidationIssue { path, invalid });
    }

    pub fn push_root(&mut self, invalid: ValidationError) {
        self.push(Path::root(), invalid);
    }

    pub fn merge(&mut self, other: ValidationReport, prefix: Option<PathSeg>) {
        for mut issue in other.issues {
            if let Some(p) = &prefix {
                issue.path = issue.path.prefixed(p.clone());
            }
            self.issues.push(issue);
        }
    }

    pub fn has_error(&self, field: &str) -> bool {
        self.issues.iter().any(|i| i.path.to_string() == field)
    }

    /// Merges another report into this one.
    pub fn extend(&mut self, other: ValidationReport) {
        self.issues.extend(other.issues);
    }

    /// Prefix all issue paths with a segment (field/index/key).
    /// Useful when validating nested structures.
    pub fn prefix(mut self, seg: PathSeg) -> Self {
        for iss in &mut self.issues {
            let mut new_segs = Vec::with_capacity(iss.path.segs.len() + 1);
            new_segs.push(seg.clone());
            new_segs.extend(iss.path.segs.iter().cloned());
            iss.path.segs = new_segs;
        }
        self
    }

    /// Prefixes all paths with a field name.
    pub fn at_field(self, name: impl Into<Cow<'static, str>>) -> Self {
        self.prefix(PathSeg::Field(name.into()))
    }
    
    /// Prefixes all paths with an array index.
    pub fn at_index(self, idx: usize) -> Self {
        self.prefix(PathSeg::Index(idx))
    }
    
    /// Prefixes all paths with a map key.
    pub fn at_key(self, key: impl Into<String>) -> Self {
        self.prefix(PathSeg::Key(key.into()))
    }

    /// Optional convenience: collapse to DRF-ish flat `{field: [msgs]}` for non-nested paths only.
    pub fn to_field_map_flat(&self) -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for iss in &self.issues {
            if iss.path.is_root() {
                m.entry("non_field_errors".into())
                    .or_default()
                    .push(iss.invalid.message.to_string());
                continue;
            }
            // Only accept single Field("x") paths for this flat view.
            if let [PathSeg::Field(f)] = iss.path.segments() {
                m.entry(f.to_string())
                    .or_default()
                    .push(iss.invalid.message.to_string());
            }
        }
        m
    }

    /// Consuming variant of `to_field_map_flat` which takes ownership.
    pub fn into_field_map_flat(self) -> BTreeMap<String, Vec<String>> {
        let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for iss in self.issues {
            if iss.path.is_root() {
                m.entry("non_field_errors".into())
                    .or_default()
                    .push(iss.invalid.message.to_string());
                continue;
            }
            if let [PathSeg::Field(f)] = iss.path.segments() {
                m.entry(f.to_string())
                    .or_default()
                    .push(iss.invalid.message.to_string());
            }
        }
        m
    }

    /// Produce a nested JSON-like map/array structure from full `Path`s.
    /// - Objects for `Field`/`Key` segments
    /// - Arrays for `Index` segments
    /// Leaves are arrays of messages (strings).
    pub fn to_nested_map(&self) -> serde_json::Value {
        let mut root = serde_json::Value::Object(serde_json::Map::new());

        fn insert_at(cur: &mut serde_json::Value, segs: &[PathSeg], msg: &serde_json::Value) {
            if segs.is_empty() {
                return;
            }
            match &segs[0] {
                PathSeg::Field(f) => {
                    let key = f.to_string();
                    if segs.len() == 1 {
                        if let Some(map) = cur.as_object_mut() {
                            let entry = map.entry(key).or_insert_with(|| serde_json::Value::Array(vec![]));
                            if let serde_json::Value::Array(arr) = entry {
                                arr.push(msg.clone());
                            }
                        }
                    } else {
                        if let Some(map) = cur.as_object_mut() {
                            let entry = map
                                .entry(key)
                                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                            if !entry.is_object() {
                                *entry = serde_json::Value::Object(serde_json::Map::new());
                            }
                            insert_at(entry, &segs[1..], msg);
                        }
                    }
                }
                PathSeg::Key(k) => {
                    let key = k.clone();
                    if segs.len() == 1 {
                        if let Some(map) = cur.as_object_mut() {
                            let entry = map.entry(key).or_insert_with(|| serde_json::Value::Array(vec![]));
                            if let serde_json::Value::Array(arr) = entry {
                                arr.push(msg.clone());
                            }
                        }
                    } else {
                        if let Some(map) = cur.as_object_mut() {
                            let entry = map
                                .entry(key)
                                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                            if !entry.is_object() {
                                *entry = serde_json::Value::Object(serde_json::Map::new());
                            }
                            insert_at(entry, &segs[1..], msg);
                        }
                    }
                }
                PathSeg::Index(idx) => {
                    if !cur.is_array() {
                        *cur = serde_json::Value::Array(vec![]);
                    }
                    if let Some(arr) = cur.as_array_mut() {
                        while arr.len() <= *idx {
                            arr.push(serde_json::Value::Null);
                        }

                        if let Some(elem) = arr.get_mut(*idx) {
                            if segs.len() == 1 {
                                if elem.is_null() {
                                    *elem = serde_json::Value::Array(vec![]);
                                }
                                if let serde_json::Value::Array(a) = elem {
                                    a.push(msg.clone());
                                }
                            } else {
                                if !elem.is_object() {
                                    *elem = serde_json::Value::Object(serde_json::Map::new());
                                }
                                insert_at(elem, &segs[1..], msg);
                            }
                        }
                    }
                }
            }
        }

        for iss in &self.issues {
            let msg = serde_json::Value::String(iss.invalid.message.to_string());
            if iss.path.is_root() {
                if let Some(map) = root.as_object_mut() {
                    let entry = map
                        .entry("non_field_errors")
                        .or_insert_with(|| serde_json::Value::Array(vec![]));
                    if let serde_json::Value::Array(arr) = entry {
                        arr.push(msg);
                    }
                }
                continue;
            }
            insert_at(&mut root, iss.path.segments(), &msg);
        }

        root
    }
}

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return write!(f, "validation passed");
        }
        write!(f, "validation failed with {} error(s)", self.issues.len())
    }
}

impl std::error::Error for ValidationReport {}

/// Structural validation trait for types that can validate themselves.
///
/// # Example
/// ```ignore
/// impl Validate for MyStruct {
///     fn validate(&self) -> Result<(), ValidationReport> {
///         let mut report = ValidationReport::empty();\n///         if self.name.is_empty() {
///             report.push_root(ValidationError::new(\"required\", \"Name is required\"));
///         }
///         if report.is_empty() {
///             Ok(())
///         } else {
///             Err(report)
///         }
///     }
/// }
/// ```
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationReport>;
}

/// Helper trait for applying validators to fields, handling Option automatically.
pub trait AsValidationTarget {
    type Target: ?Sized;
    fn as_validation_target(&self) -> Option<&Self::Target>;
}

impl<T> AsValidationTarget for Option<T> {
    type Target = T;
    fn as_validation_target(&self) -> Option<&Self::Target> {
        self.as_ref()
    }
}

impl AsValidationTarget for String {
    type Target = String;
    fn as_validation_target(&self) -> Option<&Self::Target> {
        Some(self)
    }
}

impl AsValidationTarget for &str {
    type Target = str;
    fn as_validation_target(&self) -> Option<&Self::Target> {
        Some(self)
    }
}

impl<T> AsValidationTarget for Vec<T> {
    type Target = Vec<T>;
    fn as_validation_target(&self) -> Option<&Self::Target> {
        Some(self)
    }
}

impl<T> AsValidationTarget for Box<T> {
    type Target = T;
    fn as_validation_target(&self) -> Option<&Self::Target> {
        Some(self)
    }
}

macro_rules! impl_as_validation_target {
    ($($t:ty),*) => {
        $(
            impl AsValidationTarget for $t {
                type Target = $t;
                fn as_validation_target(&self) -> Option<&Self::Target> {
                    Some(self)
                }
            }
        )*
    };
}

impl_as_validation_target!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, bool);

impl<T: Validate> Validate for Option<T> {
    fn validate(&self) -> Result<(), ValidationReport> {
        match self {
            Some(v) => v.validate(),
            None => Ok(()),
        }
    }
}

impl<T: Validate> Validate for Vec<T> {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::empty();
        for (i, v) in self.iter().enumerate() {
            if let Err(r) = v.validate() {
                report.extend(r.at_index(i));
            }
        }
        if report.is_empty() {
            Ok(())
        } else {
            Err(report)
        }
    }
}

impl<T: Validate> Validate for Box<T> {
    fn validate(&self) -> Result<(), ValidationReport> {
        self.as_ref().validate()
    }
}


/// Wrapper that validates extracted data before allowing access.
/// Use as `Valid<Json<T>>`, `Valid<Query<T>>`, or `Valid<Path<T>>`.
#[derive(Debug, Clone)]
pub struct Valid<E>(pub E);

impl<E> Valid<E> {
    /// Extract the inner value, consuming the wrapper.
    pub fn into_inner(self) -> E {
        self.0
    }
}

impl<E, T> Deref for Valid<E>
where
    E: Deref<Target = T>,
{
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Rejection type for `Valid<E>` extractor, forwarding inner rejections or validation failures.
#[derive(Debug)]
pub enum ValidRejection<R> {
    Inner(R),          // forwarded extractor rejection
    Invalid(Response), // our validation failure
}

impl<R: IntoResponse> IntoResponse for ValidRejection<R> {
    fn into_response(self) -> Response {
        match self {
            ValidRejection::Inner(r) => r.into_response(),
            ValidRejection::Invalid(resp) => resp,
        }
    }
}


fn wants_json(headers: &HeaderMap) -> bool {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*/*");

    // If client explicitly wants html, respect it.
    if accept.contains("text/html") {
        return false;
    }

    // Otherwise default to JSON (API-first).
    accept.contains("application/json") || accept.contains("*/*") || accept.contains("application/*")
}


fn drf_to_html(drf: &str) -> String {
    format!(
        "<!doctype html><html><body>\
         <h1>Validation error</h1>\
         <pre>{}</pre>\
         </body></html>",
        drf
    )
}

fn invalid_response(headers: &HeaderMap, errs: &ValidationReport) -> Response {
    let drf = errs.to_nested_map();
    if wants_json(headers) {
        (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(drf)).into_response()
    } else {
        let drf = serde_json::to_string_pretty(&drf).unwrap_or_else(|_| "{}".to_string());
        (StatusCode::UNPROCESSABLE_ENTITY, Html(drf_to_html(&drf))).into_response()
    }
}

/// ---------- FromRequestParts (Query / Path / Headers) ----------
impl<S, E, T> FromRequestParts<S> for Valid<E>
where
    S: Send + Sync,
    E: FromRequestParts<S> + Deref<Target = T> + Send,
    T: Validate,
    <E as FromRequestParts<S>>::Rejection: IntoResponse,
{
    type Rejection = ValidRejection<<E as FromRequestParts<S>>::Rejection>;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let extracted = E::from_request_parts(parts, state)
            .await
            .map_err(ValidRejection::Inner)?;

        match extracted.validate(){
            Ok(()) => Ok(Valid(extracted)),
            Err(errs) => Err(ValidRejection::Invalid(
                invalid_response(&parts.headers, &errs),
            )),
        }    
    }
}

/// ---------- FromRequest (Json / Form / Bytes) ----------
impl<S, E, T> FromRequest<S> for Valid<E>
where
    S: Send + Sync,
    E: FromRequest<S> + Deref<Target = T> + Send,
    T: Validate,
    <E as FromRequest<S>>::Rejection: IntoResponse,
{
    type Rejection = ValidRejection<<E as FromRequest<S>>::Rejection>;

    async fn from_request(
        req: Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let headers = req.headers().clone();

        let extracted = E::from_request(req, state)
            .await
            .map_err(ValidRejection::Inner)?;

        match extracted.validate(){
            Ok(()) => Ok(Valid(extracted)),
            Err(errs) => Err(ValidRejection::Invalid(
                invalid_response(&headers, &errs),
            )),
        }
        
    }
}
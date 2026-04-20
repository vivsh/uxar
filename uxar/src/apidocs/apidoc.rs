//! OpenAPI 3.0 documentation generator using schemars and openapiv3.
//!
//! This module generates OpenAPI 3.0 specifications from view metadata,
//! leveraging schemars for JSON Schema generation.

use axum::response::Html;
use indexmap::IndexMap;
use openapiv3::{
    Components, Info, MediaType, OpenAPI, Operation, Parameter, ParameterData,
    ParameterSchemaOrContent, PathItem, Paths, ReferenceOr, RequestBody, Response, Responses,
    StatusCode, Tag,
};

use crate::{
    apidocs::schema::{ComponentRegistry, SchemaConversionError},
    callables::{ArgPart, ReturnPart, ReturnSpec, ArgSpec, TypeSchema},
};

/// Available API documentation viewers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocViewer {
    Swagger,
    Redoc,
    Rapidoc,
}

/// Metadata for API documentation.
#[derive(Debug, Clone)]
pub struct ApiMeta {
    pub version: String,
    pub title: String,
    pub description: Option<String>,
    pub tags: Vec<TagInfo>,
}

impl Default for ApiMeta {
    fn default() -> Self {
        Self {
            version: "0.0.1".to_string(),
            title: "API".to_string(),
            description: None,
            tags: Vec::new(),
        }
    }
}

impl ApiMeta {
    /// Add tags to the API metadata.
    pub fn with_tags(mut self, tags: Vec<TagInfo>) -> Self {
        self.tags = tags;
        self
    }
}

/// Tag information for organizing API endpoints.
#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,
    pub description: Option<String>,
}

/// Generates OpenAPI 3.0 documentation from view metadata.
/// Uses openapiv3 crate and schemars for JSON Schema generation.
#[derive(Debug, Clone)]
pub struct ApiDocGenerator {
    pub meta: ApiMeta,
}

impl Default for ApiDocGenerator {
    fn default() -> Self {
        Self {
            meta: ApiMeta::default(),
        }
    }
}

impl ApiDocGenerator {
    /// Create a new ApiDocGenerator with the given API metadata.
    pub fn new(meta: ApiMeta) -> Self {
        Self { meta }
    }

    /// Generate OpenAPI 3.0 specification from view metadata.
    ///
    /// # Errors
    /// Returns an error if any schema conversion fails.
    pub fn generate(&self, views: &[&crate::callables::Operation]) -> Result<OpenAPI, SchemaConversionError> {
        // Create registry for schema components
        let mut registry = ComponentRegistry::new();

        // Build paths from views
        let mut paths_map: IndexMap<String, ReferenceOr<PathItem>> = IndexMap::new();

        for view in views {
            add_view_to_paths(&mut paths_map, view, &mut registry)?;
        }

        // Build tags
        let tags: Vec<Tag> = self
            .meta
            .tags
            .iter()
            .map(|t| Tag {
                name: t.name.clone(),
                description: t.description.clone(),
                external_docs: None,
                extensions: IndexMap::new(),
            })
            .collect();

        // Extract security scheme names before consuming registry
        let security_scheme_names = registry.get_security_scheme_names();

        // Get component schemas from registry
        let components_schemas = registry.into_components_schemars()?;

        // Build security schemes from registered scheme names
        let security_schemes = build_security_schemes(&security_scheme_names);

        let components = if components_schemas.is_empty() && security_schemes.is_empty() {
            None
        } else {
            Some(Components {
                schemas: components_schemas,
                security_schemes,
                ..Default::default()
            })
        };

        Ok(OpenAPI {
            openapi: "3.0.3".to_string(),
            info: Info {
                title: self.meta.title.clone(),
                description: self.meta.description.clone(),
                terms_of_service: None,
                contact: None,
                license: None,
                version: self.meta.version.clone(),
                extensions: IndexMap::new(),
            },
            servers: vec![],
            paths: Paths {
                paths: paths_map,
                extensions: IndexMap::new(),
            },
            components,
            security: None,
            tags: if tags.is_empty() { vec![] } else { tags },
            external_docs: None,
            extensions: IndexMap::new(),
        })
    }

    /// Serve API documentation viewer HTML.
    pub fn serve_doc(path: &str, viewer: DocViewer) -> Html<String> {
        match viewer {
            DocViewer::Swagger => Self::serve_swagger(path),
            DocViewer::Redoc => Self::serve_redoc(path),
            DocViewer::Rapidoc => Self::serve_rapidoc(path),
        }
    }

    fn serve_rapidoc(path: &str) -> Html<String> {
        let html = include_str!("templates/rapidoc.html").replace("###__PATH__###", path);
        Html(html)
    }

    fn serve_redoc(path: &str) -> Html<String> {
        let html = include_str!("templates/redoc.html").replace("###__PATH__###", path);
        Html(html)
    }

    fn serve_swagger(path: &str) -> Html<String> {
        let html = include_str!("templates/swagger.html").replace("###__PATH__###", path);
        Html(html)
    }

    /// Create a router serving OpenAPI docs with Swagger, Redoc, and RapiDoc viewers.
    ///
    /// # Errors
    /// Returns an error if the OpenAPI spec cannot be generated or serialized.
    pub fn views(
        &self,
        doc_url: &str,
        api_url: &str,
        views: &[&crate::callables::Operation],
    ) -> Result<axum::Router<crate::Site>, ApiDocError> {
        use axum::http::StatusCode;

        let openapi_doc = self.generate(views)?;
        let openapi_json = serde_json::to_string(&openapi_doc)
            .map_err(ApiDocError::JsonSerialization)?;

        let doc_url_owned = doc_url.to_string();
        let api_url_owned = api_url.to_string();

        Ok(axum::Router::new()
            .route(
                &api_url_owned,
                axum::routing::get(move || async move {
                    (
                        StatusCode::OK,
                        [("content-type", "application/json")],
                        openapi_json.clone(),
                    )
                }),
            )
            .route(
                &format!("{}/swagger", doc_url_owned),
                axum::routing::get({
                    let api_url = api_url_owned.clone();
                    move || async move { Self::serve_swagger(&api_url) }
                }),
            )
            .route(
                &format!("{}/redoc", doc_url_owned),
                axum::routing::get({
                    let api_url = api_url_owned.clone();
                    move || async move { Self::serve_redoc(&api_url) }
                }),
            )
            .route(
                &format!("{}/rapidoc", doc_url_owned),
                axum::routing::get({
                    let api_url = api_url_owned.clone();
                    move || async move { Self::serve_rapidoc(&api_url) }
                }),
            ))
    }
}

/// Errors that can occur when building API documentation.
#[derive(Debug, thiserror::Error)]
pub enum ApiDocError {
    #[error("schema conversion failed: {0}")]
    SchemaConversion(#[from] SchemaConversionError),
    #[error("failed to serialize OpenAPI spec: {0}")]
    JsonSerialization(#[source] serde_json::Error),
}

/// Convert TypeSchema to OpenAPI schema via JSON serialization.
fn type_schema_to_openapi(
    schema: &TypeSchema,
    registry: &mut ComponentRegistry,
) -> Result<ReferenceOr<openapiv3::Schema>, SchemaConversionError> {
    let schemars_schema = schema.schema(registry.generator_mut());
    
    let json_value = serde_json::to_value(&schemars_schema)
        .map_err(|e| SchemaConversionError::Serialization {
            name: "<inline>".to_string(),
            source: e,
        })?;
    
    convert_json_value_to_openapi(json_value, "<inline>")
}

/// Convert JSON value (from schemars) to OpenAPI schema.
fn convert_json_value_to_openapi(
    mut json_value: serde_json::Value,
    name: &str,
) -> Result<ReferenceOr<openapiv3::Schema>, SchemaConversionError> {
    if let Some(ref_str) = json_value.get("$ref").and_then(|v| v.as_str()) {
        let openapi_ref = ref_str
            .replace("#/$defs/", "#/components/schemas/")
            .replace("#/definitions/", "#/components/schemas/");
        return Ok(ReferenceOr::Reference { reference: openapi_ref });
    }
    
    transform_for_openapi(&mut json_value);
    
    let schema = serde_json::from_value::<openapiv3::Schema>(json_value)
        .map_err(|e| SchemaConversionError::Deserialization {
            name: name.to_string(),
            source: e,
        })?;
    
    Ok(ReferenceOr::Item(schema))
}

/// Transform JSON Schema to OpenAPI 3.0 in-place.
fn transform_for_openapi(val: &mut serde_json::Value) {
    if let serde_json::Value::Object(map) = val {
        if let Some(type_val) = map.get("type").and_then(|v| v.as_array()).cloned() {
            transform_type_array(map, &type_val);
        }
        
        if let Some(serde_json::Value::Object(props)) = map.get_mut("properties") {
            for (_prop_name, prop_schema) in props.iter_mut() {
                transform_for_openapi(prop_schema);
            }
        }
        
        for key in ["items", "additionalProperties", "not", "$defs", "definitions"] {
            if let Some(nested) = map.get_mut(key) {
                transform_for_openapi(nested);
            }
        }
        
        for key in ["allOf", "anyOf", "oneOf"] {
            if let Some(serde_json::Value::Array(schemas)) = map.get_mut(key) {
                for schema in schemas {
                    transform_for_openapi(schema);
                }
            }
        }
    } else if let serde_json::Value::Array(arr) = val {
        for item in arr {
            transform_for_openapi(item);
        }
    }
}

/// Transform type array to OpenAPI nullable format.
fn transform_type_array(map: &mut serde_json::Map<String, serde_json::Value>, types: &[serde_json::Value]) {
    let (has_null, non_null): (Vec<_>, Vec<_>) = types.iter()
        .partition(|v| v.as_str() == Some("null"));
    
    match non_null.len() {
        0 => {}
        1 => {
            map.insert("type".to_string(), non_null[0].clone());
            if !has_null.is_empty() {
                map.insert("nullable".to_string(), serde_json::Value::Bool(true));
            }
        }
        _ => {
            let any_of: Vec<_> = non_null.iter()
                .map(|t| serde_json::json!({"type": t}))
                .collect();
            map.remove("type");
            map.insert("anyOf".to_string(), serde_json::Value::Array(any_of));
            if !has_null.is_empty() {
                map.insert("nullable".to_string(), serde_json::Value::Bool(true));
            }
        }
    }
}

/// Add a view to the OpenAPI paths collection.
fn add_view_to_paths(
    paths: &mut IndexMap<String, ReferenceOr<PathItem>>,
    view: &crate::callables::Operation,
    registry: &mut ComponentRegistry,
) -> Result<(), SchemaConversionError> {
    let path_key = view.path.to_string();

    let path_item = paths
        .entry(path_key)
        .or_insert_with(|| ReferenceOr::Item(PathItem::default()));

    let operation = build_operation(view, registry)?;
    let method_names = view.http_methods();

    if let ReferenceOr::Item(item) = path_item {
        set_operations_for_methods(item, &method_names, operation);
    }

    Ok(())
}

/// Set operation for all HTTP methods in the MethodFilter.
fn set_operations_for_methods(item: &mut PathItem, method_names: &[&str], operation: Operation) {
    let is_multiple = method_names.len() > 1;
    for method in method_names {
        let mut op = operation.clone();
        if is_multiple {
            op.operation_id = Some(format!("{}_{}", operation.operation_id.clone().unwrap_or_default(), method.to_lowercase()));
        }
        match *method {
            "GET" => item.get = Some(op),
            "POST" => item.post = Some(op),
            "PUT" => item.put = Some(op),
            "DELETE" => item.delete = Some(op),
            "PATCH" => item.patch = Some(op),
            "HEAD" => item.head = Some(op),
            "OPTIONS" => item.options = Some(op),
            "TRACE" => item.trace = Some(op),
            _ => {}
        }
    }
}

/// Build operation from view metadata.
fn build_operation(
    view: &crate::callables::Operation,
    registry: &mut ComponentRegistry,
) -> Result<Operation, SchemaConversionError> {
    // Build parameters from both args and layer specs
    let mut parameters = build_params(&view.args, registry)?;
    
    // Process layer specs - they may contribute parameters (e.g., auth headers)
    for layer in &view.layers {
        for part in &layer.parts {
            if let Some(param) = build_layer_param(layer, part, registry)? {
                parameters.push(ReferenceOr::Item(param));
            }
        }
    }
    
    let request_body = build_request_body(&view.args, registry)?;
    let responses = build_responses(&view.returns, registry)?;
    let tags: Vec<String> = view.tags.iter().map(|s| s.to_string()).collect();

    let security = if registry.has_operation_security() {
        let scopes: Vec<String> = registry.drain_operation_scopes().collect();
        let mut sec_req = IndexMap::new();
        
        for scheme in registry.drain_operation_security() {
            sec_req.insert(scheme, scopes.clone());
        }
        
        if sec_req.is_empty() {
            None
        } else {
            Some(vec![sec_req])
        }
    } else {
        None
    };

    Ok(Operation {
        tags,
        summary: view.summary.as_ref().map(|s| s.to_string()),
        description: view.description.as_ref().map(|s| s.to_string()),
        external_docs: None,
        operation_id: Some(view.name.to_string()),
        parameters,
        request_body,
        responses,
        callbacks: IndexMap::new(),
        deprecated: false,
        security,
        servers: vec![],
        extensions: IndexMap::new(),
    })
}

/// Build parameters from argument specifications.
fn build_params(
    args: &[ArgSpec],
    registry: &mut ComponentRegistry,
) -> Result<Vec<ReferenceOr<Parameter>>, SchemaConversionError> {
    let mut result = Vec::new();

    for arg in args {
        if let Some(param) = build_param(arg, registry)? {
            result.push(ReferenceOr::Item(param));
        }
    }

    Ok(result)
}

/// Build parameter from layer specification.
fn build_layer_param(
    layer: &crate::callables::LayerSpec,
    part: &ArgPart,
    registry: &mut ComponentRegistry,
) -> Result<Option<Parameter>, SchemaConversionError> {
    let (schema, location, required) = match part {
        ArgPart::Cookie(st) => (st, "cookie", false),
        ArgPart::Header(st) => (st, "header", false),
        ArgPart::Path(st) => (st, "path", true),
        ArgPart::Query(st) => (st, "query", false),
        ArgPart::Body(_, _) => return Ok(None),
        ArgPart::Security { scheme, scopes, join_all } => {
            let scopes_str: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
            registry.register_security(scheme.to_string(), &scopes_str, *join_all);
            return Ok(None);
        }
        ArgPart::Zone | ArgPart::Ignore => return Ok(None),
    };

    let openapi_schema = type_schema_to_openapi(schema, registry)?;

    let parameter_data = ParameterData {
        name: layer.name.clone(),
        description: layer.description.clone(),
        required,
        deprecated: None,
        format: ParameterSchemaOrContent::Schema(openapi_schema),
        example: None,
        examples: IndexMap::new(),
        explode: None,
        extensions: IndexMap::new(),
    };

    let param = match location {
        "query" => Parameter::Query {
            parameter_data,
            allow_reserved: false,
            style: openapiv3::QueryStyle::Form,
            allow_empty_value: None,
        },
        "path" => Parameter::Path {
            parameter_data,
            style: openapiv3::PathStyle::Simple,
        },
        "header" => Parameter::Header {
            parameter_data,
            style: openapiv3::HeaderStyle::Simple,
        },
        "cookie" => Parameter::Cookie {
            parameter_data,
            style: openapiv3::CookieStyle::Form,
        },
        _ => return Ok(None),
    };

    Ok(Some(param))
}

/// Build a single parameter from argument specification.
fn build_param(
    arg: &ArgSpec,
    registry: &mut ComponentRegistry,
) -> Result<Option<Parameter>, SchemaConversionError> {
    let (schema, location, required) = match &arg.part {
        ArgPart::Cookie(st) => (st, "cookie", false),
        ArgPart::Header(st) => (st, "header", false),
        ArgPart::Path(st) => (st, "path", true),
        ArgPart::Query(st) => (st, "query", false),
        ArgPart::Body(_, _) => return Ok(None),
        ArgPart::Security { scheme, scopes, join_all } => {
            let scopes_str: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
            registry.register_security(scheme.to_string(), &scopes_str, *join_all);
            return Ok(None);
        }
        ArgPart::Zone | ArgPart::Ignore => return Ok(None),
    };

    let openapi_schema = type_schema_to_openapi(schema, registry)?;

    let parameter_data = ParameterData {
        name: arg.name.clone(),
        description: arg.description.clone(),
        required,
        deprecated: None,
        format: ParameterSchemaOrContent::Schema(openapi_schema),
        example: None,
        examples: IndexMap::new(),
        explode: None,
        extensions: IndexMap::new(),
    };

    let param = match location {
        "query" => Parameter::Query {
            parameter_data,
            allow_reserved: false,
            style: openapiv3::QueryStyle::Form,
            allow_empty_value: None,
        },
        "path" => Parameter::Path {
            parameter_data,
            style: openapiv3::PathStyle::Simple,
        },
        "header" => Parameter::Header {
            parameter_data,
            style: openapiv3::HeaderStyle::Simple,
        },
        "cookie" => Parameter::Cookie {
            parameter_data,
            style: openapiv3::CookieStyle::Form,
        },
        _ => return Ok(None),
    };

    Ok(Some(param))
}

/// Build request body from arguments if any body part exists.
fn build_request_body(
    args: &[ArgSpec],
    registry: &mut ComponentRegistry,
) -> Result<Option<ReferenceOr<RequestBody>>, SchemaConversionError> {
    for arg in args {
        if let ArgPart::Body(schema, content_type) = &arg.part {
            let openapi_schema = type_schema_to_openapi(schema, registry)?;

            let mut content = IndexMap::new();
            content.insert(
                content_type.to_string(),
                MediaType {
                    schema: Some(openapi_schema),
                    example: None,
                    examples: IndexMap::new(),
                    encoding: IndexMap::new(),
                    extensions: IndexMap::new(),
                },
            );

            return Ok(Some(ReferenceOr::Item(RequestBody {
                description: arg.description.clone(),
                content,
                required: true,
                extensions: IndexMap::new(),
            })));
        }
    }
    Ok(None)
}

/// Build responses from return specifications.
fn build_responses(
    returns: &[ReturnSpec],
    registry: &mut ComponentRegistry,
) -> Result<Responses, SchemaConversionError> {
    let mut responses_map: IndexMap<StatusCode, ReferenceOr<Response>> = IndexMap::new();
    let mut has_responses = false;

    for ret in returns {
        let status_code = ret.status_code.unwrap_or_else(|| default_status_for_part(&ret.part));
        let status_key = StatusCode::Code(status_code);

        match &ret.part {
            ReturnPart::Unknown => {
                has_responses = true;
                responses_map.insert(
                    status_key,
                    ReferenceOr::Item(Response {
                        description: ret.description.clone().unwrap_or_else(|| "Unknown response".to_string()),
                        headers: IndexMap::new(),
                        content: IndexMap::new(),
                        links: IndexMap::new(),
                        extensions: IndexMap::new(),
                    }),
                );
            }
            ReturnPart::Body(schema, content_type) => {
                has_responses = true;
                add_body_to_response(&mut responses_map, status_key, ret, status_code, schema, content_type, registry)?;
            }
            ReturnPart::Header(schema) => {
                has_responses = true;
                add_header_to_response(&mut responses_map, status_key, ret, status_code, schema, registry)?;
            }
            ReturnPart::Empty => {
                has_responses = true;
                responses_map
                    .entry(status_key)
                    .or_insert_with(|| create_response(ret, status_code));
            }
        }
    }

    if !has_responses {
        responses_map.insert(
            StatusCode::Code(200),
            ReferenceOr::Item(Response {
                description: "Success".to_string(),
                headers: IndexMap::new(),
                content: IndexMap::new(),
                links: IndexMap::new(),
                extensions: IndexMap::new(),
            }),
        );
    }

    Ok(Responses {
        default: None,
        responses: responses_map,
        extensions: IndexMap::new(),
    })
}

/// Get default status code for return part type.
fn default_status_for_part(part: &ReturnPart) -> u16 {
    match part {
        ReturnPart::Empty => 204,
        _ => 200,
    }
}

/// Add body content to response.
fn add_body_to_response(
    responses_map: &mut IndexMap<StatusCode, ReferenceOr<Response>>,
    status_key: StatusCode,
    ret: &ReturnSpec,
    status_code: u16,
    schema: &crate::callables::TypeSchema,
    content_type: &str,
    registry: &mut ComponentRegistry,
) -> Result<(), SchemaConversionError> {
    let openapi_schema = type_schema_to_openapi(schema, registry)?;
    
    let response = responses_map
        .entry(status_key)
        .or_insert_with(|| create_response(ret, status_code));

    if let ReferenceOr::Item(resp) = response {
        resp.content.insert(
            content_type.to_string(),
            MediaType {
                schema: Some(openapi_schema),
                example: None,
                examples: IndexMap::new(),
                encoding: IndexMap::new(),
                extensions: IndexMap::new(),
            },
        );
    }
    
    Ok(())
}

/// Add header to response.
fn add_header_to_response(
    responses_map: &mut IndexMap<StatusCode, ReferenceOr<Response>>,
    status_key: StatusCode,
    ret: &ReturnSpec,
    status_code: u16,
    schema: &crate::callables::TypeSchema,
    registry: &mut ComponentRegistry,
) -> Result<(), SchemaConversionError> {
    let openapi_schema = type_schema_to_openapi(schema, registry)?;
    
    let response = responses_map
        .entry(status_key)
        .or_insert_with(|| create_response(ret, status_code));

    if let ReferenceOr::Item(resp) = response {
        let header_name = ret.description.clone()
            .unwrap_or_else(|| "X-Custom-Header".to_string());
        resp.headers.insert(
            header_name,
            ReferenceOr::Item(openapiv3::Header {
                description: None,
                style: openapiv3::HeaderStyle::Simple,
                required: false,
                deprecated: None,
                format: ParameterSchemaOrContent::Schema(openapi_schema),
                example: None,
                examples: IndexMap::new(),
                extensions: IndexMap::new(),
            }),
        );
    }
    
    Ok(())
}

/// Create a response with proper description.
fn create_response(ret: &ReturnSpec, status_code: u16) -> ReferenceOr<Response> {
    ReferenceOr::Item(Response {
        description: ret.description.clone()
            .unwrap_or_else(|| status_description(status_code).to_string()),
        headers: IndexMap::new(),
        content: IndexMap::new(),
        links: IndexMap::new(),
        extensions: IndexMap::new(),
    })
}

/// Build security schemes from registered scheme names.
fn build_security_schemes(scheme_names: &[String]) -> IndexMap<String, ReferenceOr<openapiv3::SecurityScheme>> {
    let mut schemes = IndexMap::new();
    
    for name in scheme_names {
        let scheme = create_security_scheme(name);
        schemes.insert(name.clone(), ReferenceOr::Item(scheme));
    }
    
    schemes
}

/// Create a security scheme based on naming convention.
fn create_security_scheme(name: &str) -> openapiv3::SecurityScheme {
    let lower = name.to_lowercase();
    
    if lower.contains("bearer") || lower.contains("jwt") {
        openapiv3::SecurityScheme::HTTP {
            scheme: "bearer".to_string(),
            bearer_format: Some("JWT".to_string()),
            description: Some(format!("JWT Bearer token for {}", name)),
            extensions: IndexMap::new(),
        }
    } else if lower.contains("apikey") || lower.contains("api_key") {
        openapiv3::SecurityScheme::APIKey {
            location: openapiv3::APIKeyLocation::Header,
            name: "X-API-Key".to_string(),
            description: Some(format!("API key for {}", name)),
            extensions: IndexMap::new(),
        }
    } else if lower.contains("oauth") {
        openapiv3::SecurityScheme::OAuth2 {
            flows: openapiv3::OAuth2Flows::default(),
            description: Some(format!("OAuth2 authentication for {}", name)),
            extensions: IndexMap::new(),
        }
    } else {
        // Default to bearer auth for unknown schemes
        openapiv3::SecurityScheme::HTTP {
            scheme: "bearer".to_string(),
            bearer_format: None,
            description: Some(format!("Authentication for {}", name)),
            extensions: IndexMap::new(),
        }
    }
}

/// Get standard description for HTTP status code.
fn status_description(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "Response",
    }
}

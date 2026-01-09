use axum::{http::Method, response::Html};
use indexmap::IndexMap;
use serde_json::json;

use crate::{
    schemables::{ApiFragment, SchemaType, schema::ComponentRegistry, schema_type_to_api_schema},
    views::{ParamMeta, ReturnMeta, ViewMeta},
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
            version: "0.1.0".to_string(),
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
/// Uses the openapiv3 crate which supports OpenAPI 3.0.x specification.
#[derive(Debug, Clone)]
pub struct ApiDocGenerator {
    pub meta: ApiMeta,
    pub openapi_version: Option<String>,
}

impl Default for ApiDocGenerator {
    fn default() -> Self {
        Self {
            meta: ApiMeta::default(),
            openapi_version: None,
        }
    }
}

impl ApiDocGenerator {
    /// Default OpenAPI specification version. Currently set to 3.0.3 because the
    /// openapiv3 crate v2.2.0 only supports OpenAPI 3.0.x (uses nullable instead of type unions).
    pub const DEFAULT_OPENAPI_VERSION: &str = "3.0.3";

    /// Create a new ApiDocGenerator with the given API metadata.
    pub fn new(meta: ApiMeta) -> Self {
        Self {
            meta,
            openapi_version: None,
        }
    }

    /// Generate OpenAPI specification from view metadata.
    pub fn generate(&self, views: &[&ViewMeta]) -> openapiv3::OpenAPI {
        let mut api = openapiv3::OpenAPI::default();
        api.paths = openapiv3::Paths::default();
        api.openapi = self
            .openapi_version
            .clone()
            .unwrap_or_else(|| Self::DEFAULT_OPENAPI_VERSION.to_string());
        api.info = openapiv3::Info {
            title: self.meta.title.clone(),
            version: self.meta.version.clone(),
            description: self.meta.description.clone(),
            ..Default::default()
        };

        // Add tags metadata
        if !self.meta.tags.is_empty() {
            api.tags = self
                .meta
                .tags
                .iter()
                .map(|tag_info| openapiv3::Tag {
                    name: tag_info.name.clone(),
                    description: tag_info.description.clone(),
                    external_docs: None,
                    extensions: IndexMap::new(),
                })
                .collect();
        }

        // Create registry for schema components
        let mut registry = ComponentRegistry::new();

        for view in views {
            add_view_to_paths(&mut api.paths, view, &mut registry);
        }

        // Add components to the API spec
        let components = registry.into_components();

        // Define JWT bearer security scheme
        let mut security_schemes = IndexMap::new();
        security_schemes.insert(
            "bearerAuth".to_string(),
            openapiv3::ReferenceOr::Item(openapiv3::SecurityScheme::HTTP {
                scheme: "bearer".to_string(),
                bearer_format: Some("JWT".to_string()),
                description: Some("JWT bearer token authentication".to_string()),
                extensions: IndexMap::new(),
            }),
        );

        api.components = Some(openapiv3::Components {
            schemas: components,
            security_schemes,
            ..Default::default()
        });

        // Apply security globally
        api.security = Some(vec![openapiv3::SecurityRequirement::from([(
            "bearerAuth".to_string(),
            vec![],
        )])]);

        api
    }

    pub fn serve_doc(path: &str, viewer: DocViewer) -> Html<String> {
        match viewer {
            DocViewer::Swagger => Self::serve_swagger(path),
            DocViewer::Redoc => Self::serve_redoc(path),
            DocViewer::Rapidoc => Self::serve_rapidoc(path),
        }
    }

    fn serve_rapidoc(path: &str) -> Html<String> {
        let html = include_str!("templates/rapidoc.html").replace("###__PATH__###", path);
        Html(html.to_string())
    }

    fn serve_redoc(path: &str) -> Html<String> {
        let html = include_str!("templates/redoc.html").replace("###__PATH__###", path);
        Html(html.to_string())
    }

    fn serve_swagger(path: &str) -> Html<String> {
        let html = include_str!("templates/swagger.html").replace("###__PATH__###", path);
        Html(html.to_string())
    }

    /// Create a router serving OpenAPI docs with Swagger, Redoc, and RapiDoc viewers.
    pub fn views(
        &self,
        doc_url: &str,
        api_url: &str,
        views: &[&ViewMeta],
    ) -> axum::Router<crate::Site> {
        use axum::http::StatusCode;

        let openapi_doc = self.generate(views);
        let openapi_json = serde_json::to_string(&openapi_doc).unwrap_or_else(|_| "{}".to_string());

        let doc_url_owned = doc_url.to_string();
        let api_url_owned = api_url.to_string();

        axum::Router::new()
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
            )
    }
}

/// Add a view to the OpenAPI paths collection.
fn add_view_to_paths(
    paths: &mut openapiv3::Paths,
    view: &ViewMeta,
    registry: &mut ComponentRegistry,
) {
    let path_key = view.path.to_string();

    // Get or create path item
    let path_item = paths
        .paths
        .entry(path_key.clone())
        .or_insert_with(|| openapiv3::ReferenceOr::Item(openapiv3::PathItem::default()));

    let item = match path_item {
        openapiv3::ReferenceOr::Item(item) => item,
        _ => return, // Skip if it's a reference
    };

    // Build operation for each HTTP method
    let mut operation = build_operation(view, registry);
    let scopes = registry.drain_operation_scopes().collect::<Vec<String>>();

    if !scopes.is_empty(){
        let all_roles = registry.operation_scope_join_all;
        operation.extensions.insert(
            "x-roles".to_string(),
            json!({
                "roles": scopes,
                "mode": if all_roles { "ALL" } else { "ANY" }
            })
        );

        let description = format!("{}\n\n**Required roles ({}):** `{}`", 
            operation.description.clone().unwrap_or_default(),
            if registry.operation_scope_join_all { "ALL" } else { "ANY" },
            scopes.join(", ")
        );

        operation.description = Some(description.trim_start().to_string());
    }

    let security: Vec<String> = registry.drain_operation_security().collect();
    if !security.is_empty(){
        for schene in security {
            operation.security = Some(vec![openapiv3::SecurityRequirement::from([(
                schene,
                vec![],
            )])]);
        }
    }

    for method in &view.methods {
        set_operation_for_method(item, method, operation.clone());
    }

}

/// Set operation for a specific HTTP method in path item.
fn set_operation_for_method(
    item: &mut openapiv3::PathItem,
    method: &Method,
    operation: openapiv3::Operation,
) {
    match method.as_str() {
        "GET" => item.get = Some(operation),
        "POST" => item.post = Some(operation),
        "PUT" => item.put = Some(operation),
        "DELETE" => item.delete = Some(operation),
        "PATCH" => item.patch = Some(operation),
        "HEAD" => item.head = Some(operation),
        "OPTIONS" => item.options = Some(operation),
        "TRACE" => item.trace = Some(operation),
        _ => {} // Ignore unknown methods
    }
}

/// Build operation from view metadata.
fn build_operation(view: &ViewMeta, registry: &mut ComponentRegistry) -> openapiv3::Operation {
    let mut operation = openapiv3::Operation::default();
    operation.summary = view.summary.as_ref().map(|s| s.to_string());
    operation.description = view.description.as_ref().map(|s| s.to_string());
    operation.operation_id = Some(view.name.to_string());

    // Add tags
    if !view.tags.is_empty() {
        operation.tags = view.tags.iter().map(|t| t.to_string()).collect();
    }

    add_params(&mut operation, &view.params, registry);
    operation.responses = build_responses(&view.responses, registry);

    operation
}

/// Add path item to OpenAPI paths.
fn add_path_item(paths: &mut openapiv3::Paths, view: &ViewMeta, operation: openapiv3::Operation) {
    let path_item = openapiv3::PathItem {
        get: Some(operation),
        ..Default::default()
    };
    paths.paths.insert(
        view.path.to_string(),
        openapiv3::ReferenceOr::Item(path_item),
    );
}

/// Add parameters and request body to operation.
fn add_params(
    operation: &mut openapiv3::Operation,
    params: &[ParamMeta],
    registry: &mut ComponentRegistry,
) {
    for pm in params {
        for frag in &pm.fragments {
            // Handle Query fragments specially to flatten structs
            if let ApiFragment::Query(SchemaType::Struct(struct_schema)) = frag {
                // Flatten struct fields into individual query parameters
                for field in &struct_schema.fields {
                    let param_data = openapiv3::ParameterData {
                        name: field.name.to_string(),
                        deprecated: None,
                        description: field.about.as_ref().map(|s| s.to_string()),
                        required: is_required(&field.schema_type),
                        format: openapiv3::ParameterSchemaOrContent::Schema(
                            schema_type_to_api_schema(&field.schema_type, registry),
                        ),
                        example: None,
                        examples: IndexMap::new(),
                        explode: None,
                        extensions: IndexMap::new(),
                    };
                    let param = openapiv3::Parameter::Query {
                        parameter_data: param_data,
                        style: openapiv3::QueryStyle::Form,
                        allow_reserved: false,
                        allow_empty_value: None,
                    };
                    operation
                        .parameters
                        .push(openapiv3::ReferenceOr::Item(param));
                }
            } else {
                match to_param_or_body(pm, frag, registry) {
                    Some(either::Left(p)) => {
                        operation.parameters.push(openapiv3::ReferenceOr::Item(p))
                    }
                    Some(either::Right(rb)) => {
                        operation.request_body = Some(openapiv3::ReferenceOr::Item(rb))
                    }
                    None => {} // Skip invalid fragments
                }
            }
        }
    }
}

/// Build responses from return metadata.
fn build_responses(
    returns: &[ReturnMeta],
    registry: &mut ComponentRegistry,
) -> openapiv3::Responses {
    let mut responses = openapiv3::Responses::default();

    for rm in returns {
        add_response_fragments(rm, &mut responses, registry);
    }

    // Add default 200 response if no responses defined
    if responses.responses.is_empty() {
        let mut default_resp = openapiv3::Response::default();
        default_resp.description = "Success".to_string();
        responses.responses.insert(
            openapiv3::StatusCode::Code(200),
            openapiv3::ReferenceOr::Item(default_resp),
        );
    }

    responses
}

/// Add response fragments to responses collection.
fn add_response_fragments(
    return_meta: &ReturnMeta,
    responses: &mut openapiv3::Responses,
    registry: &mut ComponentRegistry,
) {
    for frag in &return_meta.fragments {
        if let ApiFragment::Body(stype, ctype, status) = frag {
            // All bodies in ReturnMeta are responses
            // Status defaults to 200 if not specified
            let status_code = status.unwrap_or(200);
            add_body_response(responses, stype, ctype, status_code, registry);
        }
    }
}

/// Add a body response to the responses collection.
fn add_body_response(
    responses: &mut openapiv3::Responses,
    stype: &SchemaType,
    ctype: &str,
    status: u16,
    registry: &mut ComponentRegistry,
) {
    let media_type = openapiv3::MediaType {
        schema: Some(schema_type_to_api_schema(stype, registry)),
        ..Default::default()
    };

    let status_code = openapiv3::StatusCode::Code(status);
    let mut resp = openapiv3::Response::default();
    resp.description = status_description(status);
    resp.content.insert(ctype.to_string(), media_type);

    responses
        .responses
        .insert(status_code, openapiv3::ReferenceOr::Item(resp));
}

/// Get standard description for HTTP status code.
fn status_description(status: u16) -> String {
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
    .to_string()
}

/// Convert fragment to parameter or request body.
/// All fragments in ParamMeta are request-related.
fn to_param_or_body(
    meta: &ParamMeta,
    frag: &ApiFragment,
    registry: &mut ComponentRegistry,
) -> Option<either::Either<openapiv3::Parameter, openapiv3::RequestBody>> {
    match frag {
        ApiFragment::Cookie(st) => {
            let pd = build_param_data(meta, st, registry);
            Some(either::Left(openapiv3::Parameter::Cookie {
                parameter_data: pd,
                style: openapiv3::CookieStyle::Form,
            }))
        }
        ApiFragment::Header(st) => {
            let pd = build_param_data(meta, st, registry);
            Some(either::Left(openapiv3::Parameter::Header {
                parameter_data: pd,
                style: openapiv3::HeaderStyle::Simple,
            }))
        }
        ApiFragment::Path(st) => {
            let pd = build_param_data(meta, st, registry);
            Some(either::Left(openapiv3::Parameter::Path {
                parameter_data: pd,
                style: openapiv3::PathStyle::Simple,
            }))
        }
        ApiFragment::Query(st) => {
            let pd = build_param_data(meta, st, registry);
            println!("Built query param data: {:?}", pd);
            Some(either::Left(openapiv3::Parameter::Query {
                parameter_data: pd,
                style: openapiv3::QueryStyle::Form,
                allow_reserved: false,
                allow_empty_value: None,
            }))
        }
        ApiFragment::Body(st, ctype, _) => {
            // All bodies in ParamMeta are request bodies
            Some(either::Right(build_request_body(st, ctype, registry)))
        }
        ApiFragment::Security { scheme, scopes, join_all } => {
            registry.register_security(scheme.clone(), scopes, *join_all);
            None
        }
    }
}

/// Build request body from schema type and content type.
fn build_request_body(
    st: &SchemaType,
    content_type: &str,
    registry: &mut ComponentRegistry,
) -> openapiv3::RequestBody {
    let media_type = openapiv3::MediaType {
        schema: Some(schema_type_to_api_schema(st, registry)),
        ..Default::default()
    };

    let mut content = openapiv3::Content::default();
    content.insert(content_type.to_string(), media_type);

    openapiv3::RequestBody {
        description: None,
        content,
        required: true,
        ..Default::default()
    }
}

/// Build parameter data from metadata and schema type.
fn build_param_data(
    meta: &ParamMeta,
    st: &SchemaType,
    registry: &mut ComponentRegistry,
) -> openapiv3::ParameterData {
    openapiv3::ParameterData {
        name: meta.name.to_string(),
        deprecated: None,
        description: None,
        required: is_required(st),
        format: openapiv3::ParameterSchemaOrContent::Schema(schema_type_to_api_schema(
            st, registry,
        )),
        example: None,
        examples: IndexMap::new(),
        explode: None,
        extensions: IndexMap::new(),
    }
}

/// Determine if parameter is required based on schema type.
fn is_required(st: &SchemaType) -> bool {
    !matches!(st, SchemaType::Optional { .. })
}

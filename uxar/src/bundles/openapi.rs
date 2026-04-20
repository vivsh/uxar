use std::collections::BTreeMap;

use bytes::Bytes;
use uuid::Uuid;

use crate::apidocs::{ApiDocGenerator, ApiMeta, DocViewer};
use crate::auth::AuthUser;
use crate::callables::Operation;
use crate::routes::AxumRouter;
use crate::Site;

use super::{Bundle, BundleError};

// ---------------------------------------------------------------------------
// Public configuration type
// ---------------------------------------------------------------------------

/// Configuration for the OpenAPI spec and viewer endpoints.
#[derive(Clone, Debug)]
pub struct OpenApiConf {
    /// Path where the viewer UI is served (e.g. `"/api/docs"`).
    /// Set to `None` to serve only the JSON spec without a viewer page.
    pub doc_path: Option<String>,
    /// Path where the OpenAPI JSON spec is served (e.g. `"/api/openapi.json"`).
    pub spec_path: String,
    pub meta: ApiMeta,
    pub viewer: DocViewer,
    /// Optional auth predicate for both the spec and viewer endpoints.
    /// When `Some(f)`, the request must carry a valid JWT; the extracted
    /// `AuthUser` is then passed to `f`. Returning `false` yields `403 Forbidden`.
    /// `None` (the default) leaves the endpoints publicly accessible.
    pub auth: Option<fn(&AuthUser) -> bool>,
}

impl Default for OpenApiConf {
    fn default() -> Self {
        Self {
            spec_path: "/openapi.json".to_string(),
            doc_path: None,
            meta: ApiMeta::default(),
            viewer: DocViewer::Swagger,
            auth: None,
        }
    }
}

impl OpenApiConf {
    /// Override the path for the JSON spec endpoint.
    pub fn spec(mut self, path: impl Into<String>) -> Self {
        self.spec_path = path.into();
        self
    }

    /// Enable the viewer UI at the given path.
    pub fn doc(mut self, path: impl Into<String>) -> Self {
        self.doc_path = Some(path.into());
        self
    }

    /// Set the doc viewer type (Swagger, Redoc, Rapidoc).
    pub fn viewer(mut self, viewer: DocViewer) -> Self {
        self.viewer = viewer;
        self
    }

    /// Set the API title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.meta.title = title.into();
        self
    }

    /// Set the API version string.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.meta.version = version.into();
        self
    }

    /// Set the API description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.meta.description = Some(description.into());
        self
    }

    /// Set the API tags.
    pub fn tags(mut self, tags: Vec<crate::apidocs::TagInfo>) -> Self {
        self.meta.tags = tags;
        self
    }

    /// Require authentication. The predicate receives the extracted `AuthUser`
    /// and must return `true` to allow access; `false` yields `403 Forbidden`.
    pub fn auth(mut self, pred: fn(&AuthUser) -> bool) -> Self {
        self.auth = Some(pred);
        self
    }
}

// ---------------------------------------------------------------------------
// DocEngine internals
// ---------------------------------------------------------------------------

/// A single OpenAPI doc registration inside a bundle.
/// Holds stable operation UUIDs rather than live references so that
/// `with_prefix` path updates propagate automatically before `setup` is called.
pub(super) struct DocNode {
    /// UUID of the hidden spec-route operation in `Bundle::ops`.
    spec_op_id: Uuid,
    /// UUID of the hidden viewer-route operation in `Bundle::ops`, if any.
    doc_op_id: Option<Uuid>,
    /// UUIDs of the visible operations to include in the generated spec.
    operation_ids: Vec<Uuid>,
    meta: ApiMeta,
    viewer: DocViewer,
    auth: Option<fn(&AuthUser) -> bool>,
}

/// Collects OpenAPI doc registrations from across the bundle graph.
/// Merged together in `Bundle::absorb`; finalised by `setup` in `SiteBuilder`.
pub(crate) struct DocEngine {
    nodes: Vec<DocNode>,
}

impl DocEngine {
    pub(super) fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub(super) fn register(&mut self, node: DocNode) {
        self.nodes.push(node);
    }

    pub(crate) fn merge(&mut self, other: DocEngine) {
        self.nodes.extend(other.nodes);
    }

    /// Mounts spec and viewer routes for every registered `DocNode`.
    ///
    /// Called once from `SiteBuilder::build`, after all `merge` / `with_prefix`
    /// calls are complete. Generates spec JSON synchronously so errors surface
    /// at startup. Returns a `BundleError` on doc-generation failure.
    pub(crate) fn setup(
        &self,
        router: &mut AxumRouter<Site>,
        ops: &BTreeMap<Uuid, Operation>,
    ) -> Result<(), BundleError> {
        for node in &self.nodes {
            let spec_path = ops
                .get(&node.spec_op_id)
                .map(|op| op.path.clone())
                .unwrap_or_else(|| node.spec_op_id.to_string());

            let views: Vec<&Operation> = node
                .operation_ids
                .iter()
                .filter_map(|id| ops.get(id))
                .collect();

            let spec_bytes = generate_spec(&views, &node.meta)?;

            // Both captures are plain Bytes / String — cheap clones on each request.
            let auth_pred = node.auth;
            let spec_route = {
                let b = spec_bytes;
                axum::routing::get(move |axum::extract::State(site): axum::extract::State<Site>, req: axum::extract::Request| {
                    let body = b.clone();
                    async move {
                        use axum::http::{StatusCode, header};
                        use axum::response::IntoResponse;
                        if let Some(pred) = auth_pred {
                            let (parts, _) = req.into_parts();
                            match site.authenticator().extract_user(&parts, &[], false) {
                                Err(e) => return e.into_response(),
                                Ok(user) if !pred(&user) => {
                                    return StatusCode::FORBIDDEN.into_response();
                                }
                                Ok(_) => {}
                            }
                        }
                        (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body).into_response()
                    }
                })
            };

            *router = std::mem::take(router).route(&spec_path, spec_route);

            if let Some(doc_op_id) = node.doc_op_id {
                let doc_path = ops
                    .get(&doc_op_id)
                    .map(|op| op.path.clone())
                    .unwrap_or_else(|| doc_op_id.to_string());
                let viewer_html = generate_viewer(&doc_path, &spec_path, node.viewer);
                let viewer_route = {
                    let h = viewer_html;
                    axum::routing::get(move |axum::extract::State(site): axum::extract::State<Site>, req: axum::extract::Request| {
                        let body = h.clone();
                        async move {
                            use axum::http::{StatusCode, header};
                            use axum::response::IntoResponse;
                            if let Some(pred) = auth_pred {
                                let (parts, _) = req.into_parts();
                                match site.authenticator().extract_user(&parts, &[], false) {
                                    Err(e) => return e.into_response(),
                                    Ok(user) if !pred(&user) => {
                                        return StatusCode::FORBIDDEN.into_response();
                                    }
                                    Ok(_) => {}
                                }
                            }
                            (StatusCode::OK, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
                        }
                    })
                };
                *router = std::mem::take(router).route(&doc_path, viewer_route);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Bundle::with_openapi
// ---------------------------------------------------------------------------

impl Bundle {
    /// Registers an OpenAPI JSON spec endpoint and a viewer UI for this bundle.
    ///
    /// Two hidden `Operation` markers (kind `ApiDoc`, `hidden = true`) are inserted
    /// into `meta_map` so that `with_prefix` updates their paths automatically.
    /// `DocEngine::setup` resolves them to final paths at startup.
    pub fn with_openapi(mut self, conf: OpenApiConf) -> Self {
        // Snapshot UUIDs of visible operations now; the ops map is the canonical store.
        let operation_ids: Vec<Uuid> = self
            .ops
            .values()
            .filter(|op| !op.hidden)
            .map(|op| op.id)
            .collect();

        // Create hidden marker operations and capture their UUIDs.
        let spec_op = crate::callables::Operation::from_api_doc(
            &format!("__spec__{}", conf.spec_path),
            &conf.spec_path,
        );
        let spec_op_id = spec_op.id;
        self.ops.insert(spec_op_id, spec_op);

        let doc_op_id = conf.doc_path.as_deref().map(|path| {
            let op = crate::callables::Operation::from_api_doc(
                &format!("__doc__{}", path),
                path,
            );
            let id = op.id;
            self.ops.insert(id, op);
            id
        });

        self.doc_engine.register(DocNode {
            spec_op_id,
            doc_op_id,
            operation_ids,
            meta: conf.meta,
            viewer: conf.viewer,
            auth: conf.auth,
        });
        self
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn generate_spec(views: &[&Operation], meta: &ApiMeta) -> Result<Bytes, BundleError> {
    let doc_gen = ApiDocGenerator::new(meta.clone());
    let api = doc_gen.generate(views).map_err(|e| BundleError::DocGen(e.to_string()))?;
    let vec = serde_json::to_vec(&api).map_err(|e| BundleError::DocGen(e.to_string()))?;
    Ok(Bytes::from(vec))
}

fn generate_viewer(doc_path: &str, spec_path: &str, viewer: DocViewer) -> String {
    // Compute a relative URL so the viewer works regardless of the mount prefix.
    let from_dir = doc_path.rfind('/').map(|i| &doc_path[..=i]).unwrap_or("/");
    let relative = spec_path
        .strip_prefix(from_dir)
        .unwrap_or(spec_path);
    let html = ApiDocGenerator::serve_doc(relative, viewer);
    html.0
}


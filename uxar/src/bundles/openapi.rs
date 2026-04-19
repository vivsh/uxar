use std::collections::BTreeMap;

use bytes::Bytes;
use uuid::Uuid;

use crate::apidocs::{ApiDocGenerator, ApiMeta, DocViewer};
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
    pub doc_path: String,
    /// Path where the OpenAPI JSON spec is served (e.g. `"/api/openapi.json"`).
    pub spec_path: String,
    pub meta: ApiMeta,
    pub viewer: DocViewer,
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
    /// UUID of the hidden viewer-route operation in `Bundle::ops`.
    doc_op_id: Uuid,
    /// UUIDs of the visible operations to include in the generated spec.
    operation_ids: Vec<Uuid>,
    meta: ApiMeta,
    viewer: DocViewer,
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
            let doc_path = ops
                .get(&node.doc_op_id)
                .map(|op| op.path.clone())
                .unwrap_or_else(|| node.doc_op_id.to_string());

            let views: Vec<&Operation> = node
                .operation_ids
                .iter()
                .filter_map(|id| ops.get(id))
                .collect();

            let spec_bytes = generate_spec(&views, &node.meta)?;
            let viewer_html = generate_viewer(&doc_path, &spec_path, node.viewer);

            // Both captures are plain Bytes / String — cheap clones on each request.
            let spec_route = {
                let b = spec_bytes;
                axum::routing::get(move || {
                    let body = b.clone();
                    async move {
                        use axum::http::{StatusCode, header};
                        (StatusCode::OK, [(header::CONTENT_TYPE, "application/json")], body)
                    }
                })
            };
            let viewer_route = {
                let h = viewer_html;
                axum::routing::get(move || {
                    let body = h.clone();
                    async move {
                        use axum::http::{StatusCode, header};
                        (StatusCode::OK, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], body)
                    }
                })
            };

            *router = std::mem::take(router)
                .route(&spec_path, spec_route)
                .route(&doc_path, viewer_route);
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
        let doc_op = crate::callables::Operation::from_api_doc(
            &format!("__doc__{}", conf.doc_path),
            &conf.doc_path,
        );
        let spec_op_id = spec_op.id;
        let doc_op_id = doc_op.id;

        // Hidden ops go into ops only — no name_index entry needed.
        self.ops.insert(spec_op_id, spec_op);
        self.ops.insert(doc_op_id, doc_op);

        self.doc_engine.register(DocNode {
            spec_op_id,
            doc_op_id,
            operation_ids,
            meta: conf.meta,
            viewer: conf.viewer,
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


//! Basic OpenAPI registration for a route bundle.
//!
//! Run:
//!
//! ```sh
//! cargo run --example openapi_basic
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{bundles, routes::Json};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Note {
    id: i64,
    title: String,
}

/// List notes visible to the current caller.
#[bundles::route(path = "/notes")]
async fn list_notes() -> Json<Vec<Note>> {
    Json(vec![Note {
        id: 1,
        title: "document OpenAPI".to_string(),
    }])
}

fn main() {
    let bundle = bundles::bundle! {
        list_notes,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Notes API")
            .version("0.1.0")
            .description("OpenAPI basic example")
            .spec("/api/openapi.json"),
    )
    .with_prefix("/v1");

    assert_eq!(
        bundle.reverse("list_notes", &[]),
        Some("/v1/notes".to_string())
    );
    println!("OpenAPI spec path: /v1/api/openapi.json");
}

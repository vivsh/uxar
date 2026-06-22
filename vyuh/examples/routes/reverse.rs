//! Route names, path parameters, multi-method routes, and reverse routing.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example routes_reverse
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    routes::{Json, Path, StatusCode},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Note {
    id: i64,
    title: String,
}

/// Return a note by id.
#[bundles::route(path = "/notes/{id}", name = "note_detail")]
async fn note_detail(Path(id): Path<i64>) -> Json<Note> {
    Json(Note {
        id,
        title: format!("note {id}"),
    })
}

/// Health check endpoint.
#[bundles::route(path = "/health", method = "GET", method = "HEAD")]
async fn health() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        note_detail,
        health,
    }
    .with_prefix("/v1");

    assert_eq!(
        bundle.reverse("note_detail", &[("id", "a/b c")]),
        Some("/v1/notes/a%2Fb%20c".to_string())
    );
    assert_eq!(bundle.reverse("note_detail", &[]), None);

    println!(
        "reverse note_detail: {:?}",
        bundle.reverse("note_detail", &[("id", "42")])
    );
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



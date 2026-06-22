//! Basic route registration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example routes_json_post
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
        title: "write docs".to_string(),
    }])
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        list_notes,
    };

    assert!(bundle.reverse("list_notes", &[]).is_some());
    println!("basic route registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



//! Direct route registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example routes_macroless
//! ```

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    routes::{Json, Methods, RouteConf},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Note {
    id: i64,
    title: String,
}

async fn list_notes() -> Json<Vec<Note>> {
    Json(vec![Note {
        id: 1,
        title: "direct registration".to_string(),
    }])
}

fn main() {
    let bundle = bundles::bundle([bundles::route(
        list_notes,
        RouteConf {
            name: Cow::Borrowed("list_notes"),
            path: Cow::Borrowed("/notes"),
            methods: Methods::GET,
            slash: None,
        },
    )]);

    assert_eq!(
        bundle.reverse("list_notes", &[]),
        Some("/notes".to_string())
    );
    println!("direct route registered");
}

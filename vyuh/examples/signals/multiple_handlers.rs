//! Multiple signal handlers for one data type.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example signals_multiple_handlers
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, bundles};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteChanged {
    id: i64,
}

#[bundles::signal]
async fn index_note_change(payload: Data<NoteChanged>) {
    println!("index note {}", payload.id);
}

#[bundles::signal]
async fn audit_note_change(payload: Data<NoteChanged>) {
    println!("audit note {}", payload.id);
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        index_note_change,
        audit_note_change,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("multiple signal handlers registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



//! Multiple signal handlers for one data type.
//!
//! Run:
//!
//! ```sh
//! cargo run --example signals_multiple_handlers
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

fn main() {
    let bundle = bundles::bundle! {
        index_note_change,
        audit_note_change,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("multiple signal handlers registered");
}

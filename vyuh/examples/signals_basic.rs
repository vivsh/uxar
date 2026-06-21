//! Basic signal registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example signals_basic
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    callables::Payload,
    signals::{SignalClient, SignalError},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteChanged {
    id: i64,
}

#[bundles::signal]
async fn index_note_change(payload: Payload<NoteChanged>) {
    println!("index note {}", payload.id);
}

fn submit_note_change(signals: SignalClient) -> Result<(), SignalError> {
    signals.submit(NoteChanged { id: 1 })
}

fn main() {
    let bundle = bundles::bundle! {
        index_note_change,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    let _submitter: fn(SignalClient) -> Result<(), SignalError> = submit_note_change;
    println!("basic signal registered");
}

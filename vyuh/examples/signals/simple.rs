//! Basic signal registration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example signals_simple
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, bundles,
    signals::{SignalClient, SignalError},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteChanged {
    id: i64,
}

#[bundles::signal]
async fn index_note_change(payload: Data<NoteChanged>) {
    println!("index note {}", payload.id);
}

fn submit_note_change(signals: SignalClient) -> Result<(), SignalError> {
    signals.submit(NoteChanged { id: 1 })
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        index_note_change,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    let _submitter: fn(SignalClient) -> Result<(), SignalError> = submit_note_change;
    println!("basic signal registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



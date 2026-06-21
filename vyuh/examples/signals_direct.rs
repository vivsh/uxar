//! Direct signal registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example signals_direct
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    callables::Payload,
    signals::{self, SignalClient, SignalError},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteChanged {
    id: i64,
}

async fn index_note_change(payload: Payload<NoteChanged>) {
    println!("index note {}", payload.id);
}

fn submit_note_change(signals: SignalClient) -> Result<(), SignalError> {
    signals.submit(NoteChanged { id: 2 })
}

fn main() {
    let bundle = bundles::bundle([bundles::signal::<NoteChanged, _, _>(
        index_note_change,
        signals::SignalConf::default(),
    )]);

    assert_eq!(bundle.iter_operations().count(), 1);
    let _submitter: fn(SignalClient) -> Result<(), SignalError> = submit_note_change;
    println!("direct signal registered");
}

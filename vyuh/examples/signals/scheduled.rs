//! Scheduled in-process signal submission.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example signals_scheduled
//! ```

use std::time::Duration;

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
async fn refresh_note_projection(payload: Data<NoteChanged>) {
    println!("refresh note {}", payload.id);
}

fn schedule_refresh(signals: SignalClient) -> Result<(), SignalError> {
    signals.schedule(NoteChanged { id: 3 }, Duration::from_secs(5))
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        refresh_note_projection,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    let _scheduler: fn(SignalClient) -> Result<(), SignalError> = schedule_refresh;
    println!("scheduled signal handler registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



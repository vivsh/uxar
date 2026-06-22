//! Debounced Postgres LISTEN/NOTIFY emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example emitters_pgnotify_burst_debounce
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, bundles};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteNotification {
    raw: String,
}

#[bundles::pgnotify(
    channel = "notes_changed",
    debounce_millis = 250,
    debounce = "leading_trailing"
)]
async fn publish_note_notification(payload: Data<String>) -> Data<NoteNotification> {
    Data::new(NoteNotification {
        raw: payload.to_string(),
    })
}

#[bundles::signal]
async fn record_note_notification(payload: Data<NoteNotification>) {
    println!("notification {}", payload.raw);
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        publish_note_notification,
        record_note_notification,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("debounced pgnotify emitter registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



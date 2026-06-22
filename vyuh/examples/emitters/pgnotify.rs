//! Postgres LISTEN/NOTIFY emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example emitters_pgnotify
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, bundles};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct NoteNotification {
    raw: String,
}

#[bundles::pgnotify(channel = "notes_changed")]
async fn publish_note_notification(payload: Data<String>) -> Data<NoteNotification> {
    Data::new(NoteNotification {
        raw: payload.to_string(),
    })
}

#[bundles::signal]
async fn record_note_notification(payload: Data<NoteNotification>) {
    println!("notification {}", payload.raw);
}

fn main() {
    let bundle = bundles::bundle! {
        publish_note_notification,
        record_note_notification,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("pgnotify emitter registered");
}

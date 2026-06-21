//! Periodic emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example emitters_periodic
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    callables::Payload,
    emitters::{IterCount, IterInstant},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct Heartbeat {
    count: usize,
}

#[bundles::periodic(secs = 30)]
async fn publish_heartbeat(IterCount(count): IterCount, _last: IterInstant) -> Payload<Heartbeat> {
    Heartbeat { count }.into()
}

#[bundles::signal]
async fn record_heartbeat(payload: Payload<Heartbeat>) {
    println!("heartbeat {}", payload.count);
}

fn main() {
    let bundle = bundles::bundle! {
        publish_heartbeat,
        record_heartbeat,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("periodic emitter registered");
}

//! Periodic emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example emitters_periodic
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, bundles,
    emitters::{IterCount, IterInstant},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct Heartbeat {
    count: usize,
}

#[bundles::periodic(secs = 30)]
async fn publish_heartbeat(IterCount(count): IterCount, _last: IterInstant) -> Data<Heartbeat> {
    Data::new(Heartbeat { count })
}

#[bundles::signal]
async fn record_heartbeat(payload: Data<Heartbeat>) {
    println!("heartbeat {}", payload.count);
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        publish_heartbeat,
        record_heartbeat,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("periodic emitter registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



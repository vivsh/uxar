//! Direct emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example emitters_direct
//! ```

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, bundles,
    emitters::{self, IterCount},
};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct Heartbeat {
    count: usize,
}

async fn publish_heartbeat(IterCount(count): IterCount) -> Data<Heartbeat> {
    Data::new(Heartbeat { count })
}

#[bundles::signal]
async fn record_heartbeat(payload: Data<Heartbeat>) {
    println!("heartbeat {}", payload.count);
}

fn main() {
    let bundle = bundles::bundle([
        bundles::periodic::<Heartbeat, _, _>(
            publish_heartbeat,
            emitters::PeriodicConf {
                interval: Duration::from_secs(30),
                target: emitters::EmitTarget::Signal,
            },
        ),
        __bundle_part_record_heartbeat(),
    ]);

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("direct emitter registered");
}

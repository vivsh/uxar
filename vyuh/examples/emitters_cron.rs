//! Cron emitter registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example emitters_cron
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Site, bundles};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
struct DailyTick {
    project: String,
}

#[bundles::cron(expr = "0 0 0 * * *")]
async fn publish_daily(site: Site) -> Data<DailyTick> {
    Data::new(DailyTick {
        project: site.project_dir().display().to_string(),
    })
}

#[bundles::signal]
async fn record_daily(payload: Data<DailyTick>) {
    println!("daily tick for {}", payload.project);
}

fn main() {
    let bundle = bundles::bundle! {
        publish_daily,
        record_daily,
    };

    assert_eq!(bundle.iter_operations().count(), 2);
    println!("cron emitter registered");
}

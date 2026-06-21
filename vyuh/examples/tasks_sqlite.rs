#[cfg(feature = "sqlite")]
use schemars::JsonSchema;
#[cfg(feature = "sqlite")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlite")]
use vyuh::{Data, SiteConf, bundles, db::DbConf, tasks::TaskOutcome};

#[cfg(feature = "sqlite")]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LocalJob {
    id: i64,
}

#[cfg(feature = "sqlite")]
#[bundles::task(name = "local_job")]
async fn local_job(input: Data<LocalJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("processed {}", input.id)).unwrap()
}

#[cfg(feature = "sqlite")]
fn main() {
    let _conf = SiteConf::default().database(DbConf::default());
    let _bundle = bundles::bundle! {
        local_job,
    };
}

#[cfg(not(feature = "sqlite"))]
fn main() {
    eprintln!(
        "Run this example with: cargo run -p vyuh --no-default-features --features sqlite --example tasks_sqlite"
    );
}

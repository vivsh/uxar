#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, SiteConf, bundles, db::DbConf, tasks::TaskOutcome};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LocalJob {
    id: i64,
}

#[bundles::task(name = "local_job")]
async fn local_job(input: Data<LocalJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("processed {}", input.id)).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().database(DbConf::default());
    let bundle = bundles::bundle! {
        local_job,
    };
    example_common::run_example_with_conf(conf, bundle).await
}

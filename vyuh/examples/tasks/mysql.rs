#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, SiteConf, bundles, db::DbConf, tasks::TaskOutcome};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MysqlJob {
    id: i64,
}

#[bundles::task(name = "mysql_job")]
async fn mysql_job(input: Data<MysqlJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("processed {}", input.id)).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().database(DbConf::default());
    let bundle = bundles::bundle! {
        mysql_job,
    };
    example_common::run_example_with_conf(conf, bundle).await
}

#[cfg(feature = "mysql")]
use schemars::JsonSchema;
#[cfg(feature = "mysql")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "mysql")]
use vyuh::{
    SiteConf, bundles,
    db::DbConf,
    tasks::{TaskInput, TaskOutcome},
};

#[cfg(feature = "mysql")]
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MysqlJob {
    id: i64,
}

#[cfg(feature = "mysql")]
#[bundles::task(name = "mysql_job")]
async fn mysql_job(input: TaskInput<MysqlJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("processed {}", input.id)).unwrap()
}

#[cfg(feature = "mysql")]
fn main() {
    let _conf = SiteConf::default().database(DbConf::default());
    let _bundle = bundles::bundle! {
        mysql_job,
    };
}

#[cfg(not(feature = "mysql"))]
fn main() {
    eprintln!(
        "Run this example with: cargo run -p vyuh --no-default-features --features mysql --example tasks_mysql"
    );
}

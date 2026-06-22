#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, SiteConf, bundles,
    tasks::{TaskConf, TaskOutcome},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct RenderJob {
    id: i64,
}

#[bundles::task(name = "render_report")]
async fn render_report(input: Data<RenderJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("rendered report {}", input.id)).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().tasks(TaskConf {
        concurrency: 4,
        ..TaskConf::default()
    });

    let bundle = bundles::bundle! {
        render_report,
    };
    example_common::run_example_with_conf(conf, bundle).await
}


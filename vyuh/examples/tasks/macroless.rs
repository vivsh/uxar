#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, bundles,
    tasks::{TaskHandlerConf, TaskOutcome},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ThumbnailJob {
    image_id: i64,
}

async fn build_thumbnail(input: Data<ThumbnailJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("thumbnail:{}", input.image_id)).unwrap()
}

fn app_bundle() -> bundles::Bundle {
    bundles::bundle([bundles::task(
        build_thumbnail,
        TaskHandlerConf::new("build_thumbnail"),
    )])
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = app_bundle();
    example_common::run_example(bundle).await
}

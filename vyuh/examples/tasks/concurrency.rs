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

fn main() {
    let _conf = SiteConf::default().tasks(TaskConf {
        concurrency: 4,
        ..TaskConf::default()
    });

    let _bundle = bundles::bundle! {
        render_report,
    };
}

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use vyuh::{
    Data, bundles,
    tasks::{TaskOutcome, TaskState},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ImportJob {
    source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportState {
    offset: usize,
}

#[bundles::task(name = "import_records")]
async fn import_records(state: TaskState<ImportState>, input: Data<ImportJob>) -> TaskOutcome {
    let mut state = state.0.unwrap_or(ImportState { offset: 0 });
    state.offset += 100;

    if state.offset >= 500 {
        TaskOutcome::complete(&format!("imported {}", input.source)).unwrap()
    } else {
        TaskOutcome::sleep(&state, Duration::from_secs(30)).unwrap()
    }
}

fn main() {
    let _bundle = bundles::bundle! {
        import_records,
    };
}

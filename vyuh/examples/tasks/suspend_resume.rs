#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, bundles,
    tasks::{TaskOutcome, TaskResume, TaskState},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ApprovalJob {
    request_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalState {
    submitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovalReply {
    approved: bool,
}

#[bundles::task(name = "wait_for_approval")]
async fn wait_for_approval(
    state: TaskState<ApprovalState>,
    resume: TaskResume<ApprovalReply>,
    input: Data<ApprovalJob>,
) -> TaskOutcome {
    if let Some(reply) = resume.0 {
        return TaskOutcome::complete(&format!(
            "request {} approved={}",
            input.request_id, reply.approved
        ))
        .unwrap();
    }

    let state = state.0.unwrap_or(ApprovalState { submitted: true });
    TaskOutcome::suspend(
        format!("approval:{}", input.request_id),
        &state,
        Some(&"waiting for approval"),
    )
    .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        wait_for_approval,
    };

    // With a built Site:
    // site.tasks()
    //     .resume("approval:42", ApprovalReply { approved: true })
    //     .await?;
    example_common::run_example(bundle).await
}


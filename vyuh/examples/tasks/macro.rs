#[path = "../common.rs"] mod example_common;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, bundles, tasks::TaskOutcome};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EmailJob {
    to: String,
    subject: String,
}

#[bundles::task(name = "send_email")]
async fn send_email(input: Data<EmailJob>) -> TaskOutcome {
    TaskOutcome::complete(&format!("sent {} to {}", input.subject, input.to)).unwrap()
}

fn app_bundle() -> bundles::Bundle {
    bundles::bundle! {
        send_email,
    }
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = app_bundle();

    // With a built Site:
    // site.tasks().submit(EmailJob {
    //     to: "user@example.com".into(),
    //     subject: "Welcome".into(),
    // }).await?;
    //
    // site.tasks()
    //     .submit_with(
    //         EmailJob {
    //             to: "user@example.com".into(),
    //             subject: "Welcome".into(),
    //         },
    //         vyuh::tasks::TaskOptions {
    //             priority: 10,
    //             initial_delay: Some(std::time::Duration::from_secs(300)),
    //             retry_delay: Some(std::time::Duration::from_secs(60)),
    //             lease_duration: Some(std::time::Duration::from_secs(900)),
    //             max_attempts: Some(5),
    //             identity: Some("welcome:user@example.com".into()),
    //             ..vyuh::tasks::TaskOptions::default()
    //         },
    //     )
    //     .await?;
    example_common::run_example(bundle).await
}


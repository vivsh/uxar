/// Task handler patterns and suspend/resume workflow.
///
/// Covers:
///   1. Fire-and-forget                (no return)
///   2. Fallible fire-and-forget       (Result<(), Error>)
///   3. Method-based registration      (no #[bundles::task] macro)
///   4. Suspend/resume with enum state (Result<TaskState<T>, Error>)
use vyuh::prelude::*;

// ── Input types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SendEmailJob {
    to: String,
    subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ProcessingJob {
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ApprovalRequest {
    document_id: i64,
    title: String,
    submitter: String,
}

// Resume payload sent by the approver.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum ApprovalDecision {
    Approved { approver: String },
    Rejected { approver: String, reason: String },
}

// Internal state persisted while the task is suspended.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingApproval {
    document_id: i64,
    title: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

// Pattern 1: Fire-and-forget — macro with explicit name.
#[bundles::task(name = "send_email")]
async fn send_email(input: Data<SendEmailJob>) {
    println!(
        "📧 Sending email to {} — subject: {}",
        input.to, input.subject
    );
}

// Pattern 2: Fallible — macro without explicit name (derives from fn name).
#[bundles::task]
async fn process_data(input: Data<ProcessingJob>) -> Result<(), Error> {
    println!("⚙️  Processing: {}", input.data);
    Ok(())
}

// Without the macro, register manually:
//   async fn process_data(input: Data<ProcessingJob>) -> Result<(), Error> { ... }
// Then pass to Site::build via a separate bundle:
//   let extra = bundles::task(process_data, tasks::TaskHandlerConf::new("process_data")).into_bundle();

// Pattern 3: Suspend/resume with typed output and enum state.
// `Suspension<T>` is injected automatically.
// `suspension.get()` returns Some(decision) on resume, None on first run.
#[bundles::task(name = "approve_document")]
async fn approve_document(
    suspension: Suspension<ApprovalDecision>,
    input: Data<ApprovalRequest>,
) -> Result<TaskState<ApprovalDecision>, Error> {
    match suspension.get() {
        // ── Resumed: approver has responded ──────────────────────────────
        Some(decision) => {
            match &decision {
                ApprovalDecision::Approved { approver } => {
                    println!("✅ '{}' approved by {}", input.title, approver);
                }
                ApprovalDecision::Rejected { approver, reason } => {
                    println!("❌ '{}' rejected by {} — {}", input.title, approver, reason);
                }
            }
            Ok(TaskState::complete(decision)?)
        }

        // ── First run: suspend and wait ───────────────────────────────────
        None => {
            println!(
                "⏳ '{}' (id={}) by {} — waiting for approval",
                input.title, input.document_id, input.submitter
            );

            let state = PendingApproval {
                document_id: input.document_id,
                title: input.title.clone(),
            };
            let placeholder = ApprovalDecision::Approved {
                approver: "(pending)".to_string(),
            };
            Ok(TaskState::suspend(placeholder, state)?)
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Macro-annotated handlers go directly in bundle!
    let bundle = bundles::bundle! {
        send_email,
        process_data,
        approve_document,
    };

    let conf = SiteConf::default();
    let site = Site::build(conf, bundle)
        .await
        .expect("Failed to build site");
    let tasks = site.tasks();

    // ── Fire-and-forget tasks ─────────────────────────────────────────────
    tasks
        .submit(SendEmailJob {
            to: "user@example.com".to_string(),
            subject: "Hello from Vyuh".to_string(),
        })
        .await
        .expect("submit failed");

    tasks
        .submit(ProcessingJob {
            data: "important payload".to_string(),
        })
        .await
        .expect("submit failed");

    // ── Suspend/resume tasks ──────────────────────────────────────────────
    let doc1 = tasks
        .submit(ApprovalRequest {
            document_id: 101,
            title: "Q4 Budget Proposal".to_string(),
            submitter: "alice".to_string(),
        })
        .await
        .expect("submit failed");

    let doc2 = tasks
        .submit(ApprovalRequest {
            document_id: 102,
            title: "New Hire Policy".to_string(),
            submitter: "bob".to_string(),
        })
        .await
        .expect("submit failed");

    // Allow the task engine to run and suspend the approval tasks.
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    tasks
        .resume(
            doc1,
            ApprovalDecision::Approved {
                approver: "carol".to_string(),
            },
        )
        .await
        .expect("resume failed");

    tasks
        .resume(
            doc2,
            ApprovalDecision::Rejected {
                approver: "carol".to_string(),
                reason: "Budget not aligned with targets".to_string(),
            },
        )
        .await
        .expect("resume failed");

    // Allow resumed tasks to complete.
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    Ok(())
}

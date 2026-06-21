# Tasks

Vyuh tasks are durable, coroutine-like state machines for work that must survive
process restarts, worker crashes, retries, timed delays, and delayed external
decisions. They are the right tool for emails, imports, report generation,
webhook retries, approvals, chunked processing, polling loops, and long-running
business operations where fire-and-forget signals are not enough.

The v0 design is reliability-first. Vyuh provides durable at-least-once
execution, bounded worker concurrency, persisted continuation state, timed
sleep, and topic-based suspend/resume. A task can run, save state, sleep or
suspend, and later resume from storage with new input.

## When To Use Tasks

Use tasks when work needs one or more of these properties:

- Durability across process restarts.
- Retry after transient failures.
- Delayed execution.
- Continuation over multiple attempts.
- Waiting for an external event before continuing.
- Controlled background concurrency.

Use [signals](signals.md) for in-process, fire-and-forget notifications. Use
[emitters](emitters.md) to produce scheduled or external events. Use tasks when
the work itself needs persistence, leases, and durable continuation.

## Architecture

Tasks are registered as typed handlers, submitted into a durable queue, claimed
by workers, and advanced by returning a `TaskOutcome`. A task handler is written
like ordinary async Rust, but its continuation is explicit and durable: the
handler returns the next state instead of keeping an async stack alive in memory.

Each task has:

- `input`: immutable input submitted when the task is created.
- `state`: mutable continuation state saved between attempts.
- `resume_input`: optional input supplied when a suspended task is resumed.
- `output`: latest intermediate output from a suspended task.
- `result`: final output after completion.

Each wake runs the handler with the latest durable snapshot:

```text
input + state + resume_input -> handler -> TaskOutcome
```

This is similar to a coroutine with `next` and `send`, except the continuation
state is stored in the task row. `TaskOutcome::sleep` is a timed yield.
`TaskOutcome::suspend` is a topic yield. `site.tasks().resume(topic, input)` is
the durable wake/send operation for all tasks currently suspended on that topic.

A Vyuh task is one durable handler backed by one task row. A task may sleep,
suspend, resume, retry, and maintain continuation state, but it does not
orchestrate other tasks, spawn child tasks, join parallel work, manage dependency
graphs, or execute workflow topologies. Vyuh Tasks are durable continuations for
a single unit of work.

## Durability And Reliability

Durable task execution is backed by Postgres, MySQL, or SQLite in v0. A task is
claimed by a worker, marked running, and associated with that worker through a
runner token and a `leased_until` deadline. The worker can commit the outcome
only while it still owns the row. If the worker dies, the lease eventually
expires and the running task becomes claimable by another worker.

This gives Vyuh durable at-least-once execution with stale-worker overwrite
protection. It does not provide exactly-once execution. Handlers should be
idempotent when they perform external side effects.

Postgres is the recommended backend for high-concurrency multi-worker
production deployments. MySQL is supported with transactional row claims on
InnoDB-backed deployments that support `FOR UPDATE SKIP LOCKED`. SQLite task
storage is durable and suitable for local, embedded, and single-process
deployments. For high-concurrency multi-worker production task processing,
prefer Postgres or MySQL. `MemoryTaskStore` exists for tests and local
experiments only.

Task stores use explicit SQLx row mapping and guarded SQL transitions. Outcome
commits update only rows still owned by the current runner token. SQLite claims
pending tasks through a single `UPDATE ... RETURNING` statement to reduce the
select/update race window; concurrent claim behavior is covered by the shared
store contract tests.

## Macro Sugar And Direct API

The task macro is sugar over direct bundle registration. It does not unlock
capabilities that the direct API cannot express.

Macro registration:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, bundles, tasks::TaskOutcome};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct EmailJob {
    to: String,
}

#[bundles::task(name = "send_email")]
async fn send_email(input: Data<EmailJob>) -> Result<TaskOutcome, Error> {
    TaskOutcome::complete(&format!("sent {}", input.to)).map_err(Error::other)
}

let bundle = bundles::bundle! {
    send_email,
};
```

Equivalent direct registration:

```rust
use vyuh::{bundles, tasks::TaskHandlerConf};

let bundle = bundles::bundle([bundles::task(
    send_email,
    TaskHandlerConf::new("send_email"),
)]);
```

The task name is used for registration, storage, diagnostics, and logs. It is
not a public submission selector; Vyuh submits tasks by the registered data type
and enforces one handler per data type.

## Handler Inputs

Task handlers use typed extractors:

```rust
use vyuh::{Data, tasks::{TaskResume, TaskState}};

async fn handler(
    state: TaskState<MyState>,
    resume: TaskResume<MyResumeInput>,
    input: Data<MyTaskData>,
) {
    // input is stable, state is saved between runs, resume is set by topic resume.
}
```

`Data<T>` is the submitted task data and must be the last task argument.
`TaskState<T>` and `TaskResume<T>` are optional typed values and can appear
before it. `Site` can also be extracted when the handler needs framework access.

State and resume input are independent. `Data<T>` is the immutable original
request. `TaskState<T>` is the task's durable memory. `TaskResume<T>` is the
latest input supplied by a topic resume. That separation lets a task keep its
original purpose, remember progress, and accept new wake-up data without
rewriting the original input.

## Outcomes

Task handlers usually return `Result<TaskOutcome, vyuh::Error>`:

- `Complete { result }`: finish successfully and store the final result.
- `Sleep { state, delay }`: save state and wake after a delay.
- `Suspend { topic, state, output }`: save state/output and wait for a topic
  resume.
- `Retry { delay, error }`: record the error and try again later.
- `Fail { error }`: finish as failed.

Helper constructors serialize ordinary Rust values:

```rust
use std::time::Duration;
use vyuh::tasks::TaskOutcome;

let done = TaskOutcome::complete(&"ok")?;
let sleeping = TaskOutcome::sleep(&state, Duration::from_secs(30))?;
let suspended = TaskOutcome::suspend("approval:42", &state, Some(&"waiting for approval"))?;
```

An `Err(vyuh::Error)` is terminal for that task attempt and is committed as a
failed task outcome. Retry is never inferred from `ErrorKind`; return
`TaskOutcome::retry(...)` when the task should be tried again later:

```rust
async fn send_email(Data(job): Data<EmailJob>) -> Result<TaskOutcome, vyuh::Error> {
    match deliver(&job).await {
        Ok(()) => TaskOutcome::complete(&"sent").map_err(vyuh::Error::other),
        Err(err) if err.is_transient() => {
            Ok(TaskOutcome::retry(Some(std::time::Duration::from_secs(60)), err.to_string()))
        }
        Err(err) => Err(vyuh::Error::unavailable(err.to_string())),
    }
}
```

See [Errors](errors.md) for the application error boundary.

## Suspend And Resume

Suspension is for tasks that cannot continue until something else happens:
approval, payment confirmation, a webhook, a file upload, or another application
event.

When a task suspends, it names a topic:

```rust
TaskOutcome::suspend("approval:42", &state, Some(&"waiting for approval"))?
```

The task becomes durable and inactive. It will not run again until application
code resumes that topic:

```rust
let resumed = site
    .tasks()
    .resume("approval:42", ApprovalReply { approved: true })
    .await?;
```

Resume wakes all tasks currently suspended on the topic and gives each one the
same resume input. Topic resume does not retain events. If no task is currently
waiting on the topic, `resume` returns `0`.

Suspension is durable. A suspended task does not consume a worker slot, does not
hold a Rust future in memory, and does not depend on the original process
remaining alive. The next run sees the saved `TaskState<T>` plus the
`TaskResume<T>` value supplied by the resume call.

## Sleep And Continuation

Sleep is for timed continuation. The handler saves new state, chooses a delay,
and Vyuh wakes the task after that delay:

```rust
TaskOutcome::sleep(&state, Duration::from_secs(30))?
```

Use sleep for polling external systems, chunked imports, slow retries with
progress, and staged work where the next step is time-based rather than
event-based.

Sleep does not write `output`. The saved `state` is the task's private durable
progress. Use `Suspend` when the task should also yield externally visible
intermediate output while waiting.

Sleep is also durable. If the process exits while a task is sleeping, the task
remains pending with a future `ready_at` time and can be claimed after that time
when workers are running again.

## Submit Tasks

Submit by registered data type:

```rust
site.tasks().submit(EmailJob {
    to: "user@example.com".into(),
}).await?;
```

Use `submit_with` when the caller needs an initial delay, identity, retry
policy, lease duration, max attempts, or initial state:

```rust
use std::time::Duration;
use vyuh::tasks::TaskOptions;

site.tasks()
    .submit_with(
        EmailJob { to: "user@example.com".into() },
        TaskOptions {
            initial_delay: Some(Duration::from_secs(300)),
            retry_delay: Some(Duration::from_secs(60)),
            lease_duration: Some(Duration::from_secs(900)),
            max_attempts: Some(5),
            identity: Some("welcome:user@example.com".into()),
            ..TaskOptions::default()
        },
    )
    .await?;
```

`TaskOptions::identity` is an optional submit-side duplicate key. When set, Vyuh
allows only one active task with that identity. Active means `pending`,
`running`, or `suspended`; terminal `succeeded` and `failed` tasks release the
identity. This prevents duplicate active submissions, but it does not provide
exactly-once execution or make handler side effects idempotent.

Submit does not provide a separate scheduling API. Initial delayed execution is
`TaskOptions::initial_delay`, timed continuation belongs in
`TaskOutcome::sleep`, and recurring creation belongs in emitters.

## Concurrency And Leases

`TaskConf.concurrency` is the maximum number of tasks a runner executes in
parallel. `TaskConf.batch_size` controls how many tasks a runner claims at a
time. `TaskConf.lease_duration_ms` controls the default lease duration for running
tasks.

```rust
use vyuh::{SiteConf, tasks::TaskConf};

let conf = SiteConf::default().tasks(TaskConf {
    concurrency: 4,
    batch_size: 100,
    lease_duration_ms: 300_000,
    ..TaskConf::default()
});
```

Lease reclaim is conservative: an expired running task is eligible to be claimed
again and may run another time. Use `TaskOptions::lease_duration` for task
instances that are expected to run longer than the default lease. A longer lease
reduces premature duplicate execution for slow work, but it also delays reclaim
when the worker really has crashed.

## Stores

The Postgres, MySQL, and SQLite stores keep the task lifecycle in one durable
row and index the hot paths for claiming pending work, resuming suspended
topics, and reclaiming expired running leases. The old separate output table is
not part of the v0 task execution model.

Use Postgres for production multi-worker deployments by default. Use MySQL when
the rest of the application is already on MySQL and the deployment uses an
InnoDB-compatible server with row-locking support. Use SQLite when the app is
embedded, local, single-process, or needs a durable queue without running a
separate database service.

Postgres stores tasks in `vyuh.tasks`; MySQL and SQLite store tasks in
`vyuh_tasks`. All durable stores enforce active task identity uniqueness.

SQLite notes:

- Claims are durable and protected by guarded updates.
- Timestamp comparisons are exercised by the task store contract tests.
- SQLite is not positioned as the high-concurrency production worker backend.
- For multiple worker processes or heavy parallel task processing, use Postgres
  or MySQL.

## Examples

- [`tasks_basic.rs`](../vyuh/examples/tasks_basic.rs): macro task registration
  and typed submit shape.
- [`tasks_direct.rs`](../vyuh/examples/tasks_direct.rs): equivalent direct task
  registration through `bundles::task`.
- [`tasks_sleep.rs`](../vyuh/examples/tasks_sleep.rs): continuation state with a
  timed wake.
- [`tasks_suspend_resume.rs`](../vyuh/examples/tasks_suspend_resume.rs): topic
  suspension and explicit resume.
- [`tasks_concurrency.rs`](../vyuh/examples/tasks_concurrency.rs): configuring
  max parallel task execution.
- [`tasks_sqlite.rs`](../vyuh/examples/tasks_sqlite.rs): SQLite-backed local
  task configuration.
- [`tasks_mysql.rs`](../vyuh/examples/tasks_mysql.rs): MySQL-backed task
  configuration.

## Failure Modes

- Unregistered task data types return `TaskError::TaskNotFound`.
- `Valid<Data<T>>` validation failures during execution are committed as failed
  task outcomes.
- Handler `Err(vyuh::Error)` values are committed as failed task outcomes.
- Stale workers cannot overwrite tasks they no longer own.
- Retried tasks become failed when `max_attempts` is reached.
- Topic resume wakes current waiters only.

## Current Limitations

- Durable task execution is implemented for Postgres, MySQL, and SQLite in v0.
- No exactly-once guarantee.
- No retained topic events.
- No durable per-attempt audit history.
- No multi-task workflow orchestration, child tasks, joins, branches,
  dependency graphs, or workflow execution engine.
- SQLite is intended for embedded/local/single-process task execution.

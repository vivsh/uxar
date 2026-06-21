# Signals

Signals are typed, in-process notifications for decoupling application code.
They are fire-and-forget: Vyuh does not guarantee delivery, ordering,
durability, retries, or handler completion. Use tasks when work must be durable
or observable as a unit of background execution.

## Overview

A signal is dispatched by Rust data type. Every handler registered for that data
type is eligible to run when the signal is submitted.

Signals are useful for lightweight local fan-out:

- update an in-memory projection after a route succeeds,
- notify several local handlers after an emitter produces data,
- split non-critical side effects out of request handlers.

Signals are not a queue. If the process exits, pending and scheduled signals can
be lost.

## Macro Sugar and Direct API

`#[bundles::signal]` is sugar over direct bundle registration with
`bundles::signal(handler, SignalConf::default())`.

Use the macro for ordinary handlers:

```rust
#[bundles::signal]
async fn index_note_change(Data(event): Data<NoteChanged>) -> Result<(), vyuh::Error> {
    println!("note {} changed", event.id);
    Ok(())
}

let bundle = bundles::bundle! {
    index_note_change,
};
```

The equivalent direct registration is:

```rust
let bundle = bundles::bundle([bundles::signal::<NoteChanged, _, _>(
    index_note_change,
    signals::SignalConf::default(),
)]);
```

The macro does not add a unique runtime capability. Prefer the direct API for
generated, conditional, or table-driven registration.

## Data

Signal data types must implement Vyuh's data bounds: they are serializable,
deserializable, schema-capable, sendable, syncable, and `'static`.

```rust
#[derive(Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
struct NoteChanged {
    id: i64,
}
```

Handlers extract signal data with `Data<T>`. The data extractor must be the last
handler argument.

```rust
#[bundles::signal]
async fn audit_note_change(site: Site, Data(event): Data<NoteChanged>) -> Result<(), vyuh::Error> {
    tracing::info!("note {} changed in {:?}", event.id, site.project_dir());
    Ok(())
}
```

`Site` and other site-derived extractors can appear before `Data<T>`.
Handler logic should return `vyuh::Error` when it can fail. Vyuh logs handler
errors and continues dispatching later handlers.

## Submitting Signals

Application code submits signals through the site-scoped signal client:

```rust
site.signals().submit(NoteChanged { id: 42 })?;
```

`submit` validates that at least one handler exists for the data type, then
queues dispatch on the site runtime and returns. Handler errors are logged and
are not returned to the submitter.

## Scheduling Signals

Signals can be scheduled for delayed in-process dispatch:

```rust
site.signals()
    .schedule(NoteChanged { id: 42 }, std::time::Duration::from_secs(5))?;
```

Scheduled signals are cancelled when the site shuts down. They are not persisted
and are not retried.

## Bundles

Signal handlers are registered as `BundlePart` values. Macro signal handlers
and direct `bundles::signal(...)` registration produce the same kind of bundle
part.

When bundles are merged, handlers for the same data type are appended. A single
submitted value can therefore fan out to multiple handlers.

See [Bundles](bundles.md) for `BundlePart`, `bundle!`, cross-module bundle
organization, validation, composition behavior, and the general patch API.

## Emitters

Emitters can produce typed data and target signals. Cron, periodic, and
notification-driven emitters use the same signal dispatch path. Signals remain
fire-and-forget even when the source is an emitter.

## Examples

Run the signal examples in increasing complexity:

```sh
cargo run --example signals_basic
cargo run --example signals_direct
cargo run --example signals_multiple_handlers
cargo run --example signals_scheduled
```

- `signals_basic`: macro handler registration and immediate submit API.
- `signals_direct`: equivalent direct `bundles::signal(...)` registration.
- `signals_multiple_handlers`: one data type with multiple handlers.
- `signals_scheduled`: delayed in-process scheduling.

## Failure Modes

- `SignalError::NotFound`: no handler is registered for the submitted data
  type.
- Handler failure: the failure is logged and dispatch continues to later
  handlers.
- Process shutdown: pending and scheduled signals can be cancelled or lost.
- Data type mismatch: usually indicates manually constructed data was
  dispatched with the wrong type.

## Best Practices

- Keep signal handlers small and non-critical.
- Use stable data structs instead of primitive values for public subsystem
  boundaries.
- Use tasks for durable work, retries, delayed persistence, or continuation state.
- Return `vyuh::Error` for application failures inside handlers; keep
  `SignalError` for signal machinery.
- Treat scheduling as a convenience timer, not as a queue.
- Prefer direct registration when signal handlers are generated or conditional.

## Current Limitations

- Signals are in-process only.
- Dispatch is type-based, not name-based or topic-based.
- There is no delivery acknowledgement.
- There is no ordering guarantee across handlers or submissions.
- Debounce is not part of the v0 signal surface.

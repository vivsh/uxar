# Emitters

Emitters are typed in-process event sources. They run on the site runtime,
produce `Data<T>` values, and dispatch that data to another subsystem.
For v0, the public target is signals.

Emitters are not durable queues. Missed cron or periodic ticks are not replayed,
Postgres notifications are not persisted by Vyuh, and handler failures are
logged rather than retried. Use tasks when work must be durable or observable as
a unit of background execution.

## Overview

Vyuh has three public emitter sources:

- `cron`: produce data from a cron schedule.
- `periodic`: produce data at a fixed interval.
- `pgnotify`: produce data from a Postgres `LISTEN`/`NOTIFY` channel.

Emitter handlers return `Data<T>`. With the default signal target, the
data type `T` must have at least one registered signal handler or signal
dispatch logs `SignalError::NotFound`.
Handlers that can fail should return `Result<Data<T>, vyuh::Error>`.

## Macro Sugar and Direct API

Emitter macros are sugar over direct bundle registration APIs:

- `#[bundles::cron]` maps to `bundles::cron(handler, CronConf)`.
- `#[bundles::periodic]` maps to `bundles::periodic(handler, PeriodicConf)`.
- `#[bundles::pgnotify]` maps to `bundles::pgnotify(handler, PgNotifyConf)`.

Use the macro for ordinary static emitters:

```rust
#[bundles::periodic(secs = 30)]
async fn publish_heartbeat(IterCount(count): IterCount) -> Data<Heartbeat> {
    Data::new(Heartbeat { count })
}
```

The equivalent direct registration is:

```rust
let part = bundles::periodic::<Heartbeat, _, _>(
    publish_heartbeat,
    emitters::PeriodicConf {
        interval: std::time::Duration::from_secs(30),
        target: emitters::EmitTarget::Signal,
    },
);
```

The macro path does not add a unique runtime capability. Prefer direct
registration when emitters are generated, conditional, or table-driven.

## Handler Signatures

Emitter handlers can extract `Site`, `IterCount`, and `IterInstant` before
returning `Data<T>`.

```rust
#[bundles::periodic(secs = 60)]
async fn publish_minute(site: Site, IterCount(count): IterCount) -> Result<Data<MinuteTick>, vyuh::Error> {
    Ok(Data::new(MinuteTick {
        count,
        project: site.project_dir().display().to_string(),
    }))
}
```

`IterCount` is the number of times that emitter work item has fired. It starts
at `0`. `IterInstant` is the previous fire time, or `None` for the first run.

## Cron

Cron emitters use the `cron` crate schedule syntax. Macro cron expressions are
parsed at compile time.

```rust
#[bundles::cron(expr = "0 0 0 * * *")]
async fn publish_daily() -> Data<DailyTick> {
    Data::new(DailyTick)
}
```

Direct registration uses `CronConf`:

```rust
let part = bundles::cron::<DailyTick, _, _>(
    publish_daily,
    emitters::CronConf {
        expr: "0 0 0 * * *".to_string(),
        target: emitters::EmitTarget::Signal,
    },
);
```

Cron emitters run in-process. If the site is stopped during a scheduled time,
Vyuh does not replay that tick when the site starts again.

## Periodic

Periodic emitters run on a fixed in-process interval. The macro accepts `secs`,
`millis`, or both.

```rust
#[bundles::periodic(secs = 1, millis = 500)]
async fn publish_queue_tick() -> Data<QueueTick> {
    Data::new(QueueTick)
}
```

Direct registration uses `PeriodicConf`:

```rust
let part = bundles::periodic::<QueueTick, _, _>(
    publish_queue_tick,
    emitters::PeriodicConf {
        interval: std::time::Duration::from_millis(1500),
        target: emitters::EmitTarget::Signal,
    },
);
```

Periodic emitters are timers, not queues. Slow handlers and process shutdown can
delay or lose ticks.

## PgNotify

PgNotify emitters listen to a Postgres channel and receive the raw notification
data as `Data<String>`.

```rust
#[bundles::pgnotify(channel = "notes_changed")]
async fn publish_note_notification(payload: Data<String>) -> Data<NoteNotification> {
    Data::new(NoteNotification {
        raw: payload.to_string(),
    })
}
```

Direct registration uses `PgNotifyConf`:

```rust
let part = bundles::pgnotify::<NoteNotification, _, _>(
    publish_note_notification,
    emitters::PgNotifyConf {
        channel: "notes_changed".to_string(),
        target: emitters::EmitTarget::Signal,
        debounce: None,
    },
);
```

PgNotify is Postgres-only. MySQL and SQLite builds can use cron and periodic
emitters, but `pgnotify` requires Postgres `LISTEN`/`NOTIFY`.

### PgNotify Debounce

PgNotify emitters can debounce bursty notifications before running the handler:

```rust
#[bundles::pgnotify(
    channel = "notes_changed",
    debounce_millis = 250,
    debounce = "leading_trailing"
)]
async fn publish_note_notification(payload: Data<String>) -> Data<NoteNotification> {
    Data::new(NoteNotification {
        raw: payload.to_string(),
    })
}
```

Supported modes are:

| Mode | Behavior |
| --- | --- |
| `leading` | run immediately for the first notification and suppress the rest of the window |
| `trailing` | run once after a quiet window with the last payload |
| `leading_trailing` | run immediately, then run once more with the last payload only when more notifications arrived |

If `debounce_millis` or `debounce_secs` is set without `debounce`, the mode
defaults to `trailing`. Debounce is scoped to one PgNotify emitter
registration, not shared globally by channel name.

When a PgNotify emitter produces the same `Data<T>` as a cron or periodic
emitter, every raw notification still postpones that timer fallback. This means
periodic or cron fallback runs when no notifications arrive, but is pushed back
while notifications are active, even if debounce suppresses immediate handler
execution.

Pending trailing emissions are not flushed on shutdown.

## Bundles

Emitters are registered as `BundlePart` values. Macro emitters and direct
`bundles::cron`, `bundles::periodic`, or `bundles::pgnotify` registration
produce the same kind of bundle part.

Emitter registrations are unique by emitted data type and emitter source kind.
Registering two periodic emitters for the same data type, for example,
is rejected during bundle validation.

See [Bundles](bundles.md) for `BundlePart`, `bundle!`, cross-module bundle
organization, validation, composition behavior, and the general patch API.

## Examples

Run the emitter examples in increasing complexity:

```sh
cargo run --example emitters_periodic
cargo run --example emitters_direct
cargo run --example emitters_cron
cargo run --example emitters_pgnotify
cargo run --example emitters_pgnotify_debounce
```

- `emitters_periodic`: macro-based periodic emitter and signal handler.
- `emitters_direct`: equivalent direct periodic registration.
- `emitters_cron`: cron emitter using `Site` extraction.
- `emitters_pgnotify`: Postgres notification emitter registration.
- `emitters_pgnotify_debounce`: PgNotify emitter with leading and trailing
  debounce.

## Failure Modes

- Invalid cron expression: macro registration fails at compile time; direct
  registration records a bundle error.
- Invalid periodic interval attributes: the macro requires `secs`, `millis`, or
  both.
- Duplicate emitter: the same data type and source kind is already
  registered.
- PgNotify setup failure: startup fails if Postgres notification listening
  cannot be established.
- Handler failure: the error is logged and the emitter continues running.
- Signal target without a handler: dispatch logs `SignalError::NotFound`.

## Best Practices

- Return stable data structs and handle the real work in signal handlers.
- Keep emitter handlers small; use them to produce events, not durable work.
- Return `vyuh::Error` for application failures; keep `EmitterError` for
  emitter registration and source machinery.
- Use direct registration for generated or conditional emitter lists.
- Keep pgnotify data parsing explicit and small.
- Use tasks for durable continuations, retries, persistence, or job observability.

## Current Limitations

- Emitters are in-process only.
- Cron and periodic ticks are not persisted or replayed.
- PgNotify is Postgres-only.
- The public v0 target is signals.

# Emitters

Emitters are typed in-process event sources. They run on the site runtime,
produce `Payload<T>` values, and dispatch those payloads to another subsystem.
For v0, the public target is signals.

Emitters are not durable queues. Missed cron or periodic ticks are not replayed,
Postgres notifications are not persisted by Vyuh, and handler failures are
logged rather than retried. Use tasks when work must be durable or observable as
a unit of background execution.

## Overview

Vyuh has three public emitter sources:

- `cron`: produce a payload from a cron schedule.
- `periodic`: produce a payload at a fixed interval.
- `pgnotify`: produce a payload from a Postgres `LISTEN`/`NOTIFY` channel.

Emitter handlers return `Payload<T>`. With the default signal target, the
payload type `T` must have at least one registered signal handler or signal
dispatch logs `SignalError::NotFound`.

## Macro Sugar and Direct API

Emitter macros are sugar over direct bundle registration APIs:

- `#[bundles::cron]` maps to `bundles::cron(handler, CronConf)`.
- `#[bundles::periodic]` maps to `bundles::periodic(handler, PeriodicConf)`.
- `#[bundles::pgnotify]` maps to `bundles::pgnotify(handler, PgNotifyConf)`.

Use the macro for ordinary static emitters:

```rust
#[bundles::periodic(secs = 30)]
async fn publish_heartbeat(IterCount(count): IterCount) -> Payload<Heartbeat> {
    Heartbeat { count }.into()
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

Emitter handlers can extract `Site`, `IterCount`, and `IterInstant` before the
returned payload.

```rust
#[bundles::periodic(secs = 60)]
async fn publish_minute(site: Site, IterCount(count): IterCount) -> Payload<MinuteTick> {
    MinuteTick {
        count,
        project: site.project_dir().display().to_string(),
    }
    .into()
}
```

`IterCount` is the number of times that emitter work item has fired. It starts
at `0`. `IterInstant` is the previous fire time, or `None` for the first run.

## Cron

Cron emitters use the `cron` crate schedule syntax. Macro cron expressions are
parsed at compile time.

```rust
#[bundles::cron(expr = "0 0 0 * * *")]
async fn publish_daily() -> Payload<DailyTick> {
    DailyTick.into()
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
async fn publish_queue_tick() -> Payload<QueueTick> {
    QueueTick.into()
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
payload as `Payload<String>`.

```rust
#[bundles::pgnotify(channel = "notes_changed")]
async fn publish_note_notification(payload: Payload<String>) -> Payload<NoteNotification> {
    NoteNotification {
        raw: payload.to_string(),
    }
    .into()
}
```

Direct registration uses `PgNotifyConf`:

```rust
let part = bundles::pgnotify::<NoteNotification, _, _>(
    publish_note_notification,
    emitters::PgNotifyConf {
        channel: "notes_changed".to_string(),
        target: emitters::EmitTarget::Signal,
    },
);
```

PgNotify is Postgres-only. MySQL and SQLite builds can use cron and periodic
emitters, but `pgnotify` requires Postgres `LISTEN`/`NOTIFY`.

## Bundles

Emitters are registered as `BundlePart` values. Macro emitters and direct
`bundles::cron`, `bundles::periodic`, or `bundles::pgnotify` registration
produce the same kind of bundle part.

Emitter registrations are unique by emitted payload type and emitter source
kind. Registering two periodic emitters for the same payload type, for example,
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
```

- `emitters_periodic`: macro-based periodic emitter and signal handler.
- `emitters_direct`: equivalent direct periodic registration.
- `emitters_cron`: cron emitter using `Site` extraction.
- `emitters_pgnotify`: Postgres notification emitter registration.

## Failure Modes

- Invalid cron expression: macro registration fails at compile time; direct
  registration records a bundle error.
- Invalid periodic interval attributes: the macro requires `secs`, `millis`, or
  both.
- Duplicate emitter: the same payload type and source kind is already
  registered.
- PgNotify setup failure: startup fails if Postgres notification listening
  cannot be established.
- Handler failure: the error is logged and the emitter continues running.
- Signal target without a handler: dispatch logs `SignalError::NotFound`.

## Best Practices

- Return stable payload structs and handle the real work in signal handlers.
- Keep emitter handlers small; use them to produce events, not durable work.
- Use direct registration for generated or conditional emitter lists.
- Keep pgnotify payload parsing explicit and small.
- Use tasks for durable continuations, retries, persistence, or job observability.

## Current Limitations

- Emitters are in-process only.
- Cron and periodic ticks are not persisted or replayed.
- PgNotify is Postgres-only.
- The public v0 target is signals.

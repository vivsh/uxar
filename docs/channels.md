# Channels

Vyuh channels are live pub/sub delivery for clients. Use them when browser or
machine clients need to receive application events over SSE, WebSocket, or long
polling.

Channels are not durable work queues. Use [Tasks](tasks.md) for durable
background work and [Signals](signals.md) for in-process handler fanout.

Use channels for client-facing live updates over SSE, WebSocket, or long
polling. Do not use channels for task queues, exactly-once delivery, or
application-internal fanout.

## Mental Model

| Need | Use |
| --- | --- |
| Client-facing live events | `channels` |
| In-process application event fanout | `signals` |
| Scheduled or external event sources | `emitters` |
| Durable retryable work | `tasks` |
| Site-lifetime state and workers | `services` |

Applications own topic authorization. A route can extract `AuthUser`, `ApiKey`,
services, or database access, decide which topics are allowed, then subscribe
through `ChannelRef`.

## Publishing

Extract `ChannelRef` and publish typed `Data<T>`:

```rust
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, channels::ChannelRef};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct OrderUpdated {
    order_id: i64,
    status: String,
}

async fn publish(
    channels: ChannelRef,
    Data(input): Data<OrderUpdated>,
) -> Result<(), Error> {
    channels
        .publish("orders.updated", Data::new(input.as_ref().clone()))
        .await
        .map_err(Error::from)?;
    Ok(())
}
```

Use `publish_json(...)` when the message is already a JSON value.

## Subscribing

SSE, WebSocket, and long polling are explicit route handlers. Vyuh does not
auto-mount channel routes.

```rust
use vyuh::{Error, auth::AuthUser, channels::{ChannelRef, ChannelSse}};

async fn events(user: AuthUser, channels: ChannelRef) -> Result<ChannelSse, Error> {
    let topic = format!("users.{}/orders", user.key);
    channels.sse(topic).await.map_err(Error::from)
}
```

WebSocket routes need Axum's upgrade extractor:

```rust
use axum::extract::ws::WebSocketUpgrade;
use vyuh::{Error, channels::{ChannelRef, ChannelWebSocket}};

async fn ws(
    upgrade: WebSocketUpgrade,
    channels: ChannelRef,
) -> Result<ChannelWebSocket, Error> {
    channels.websocket(upgrade, "orders.updated").await.map_err(Error::from)
}
```

Long polling accepts an optional cursor and returns JSON:

```rust
use serde::Deserialize;
use vyuh::{Error, channels::{ChannelCursor, ChannelLongPoll, ChannelRef}, routes::Query};

#[derive(Deserialize)]
struct PollQuery {
    after: Option<ChannelCursor>,
}

async fn poll(
    channels: ChannelRef,
    Query(query): Query<PollQuery>,
) -> Result<ChannelLongPoll, Error> {
    channels
        .long_poll("orders.updated", query.after)
        .await
        .map_err(Error::from)
}
```

## Replay And Backpressure

Channels provide live delivery with bounded replay. `ChannelCursor` is opaque;
clients should pass it back unchanged.

The local backend keeps recent events in memory. It is fast and single-process.
It is not durable and does not deliver across multiple server processes.

Subscribers have bounded queues. The default slow-subscriber policy disconnects
subscribers whose queues fill, so publishing never waits indefinitely on a slow
client.

## Configuration

```rust
use vyuh::{SiteConf, channels::ChannelConf};

let conf = SiteConf::default().channels(ChannelConf {
    retention_events: 20_000,
    subscriber_queue: 512,
    ..ChannelConf::default()
});
```

Important limits include `max_topics_per_subscribe`, `max_topic_len`,
`max_message_bytes`, `replay_limit`, and `long_poll_timeout_ms`.

## Custom Backends

`LocalChannelBackend` is the default implementation. The public
`ChannelBackend` trait is shaped so Redis-like backends can later provide
cross-process pub/sub and replay storage.

Custom backends should preserve Vyuh's channel semantics: bounded replay,
opaque cursors, non-blocking publish, and explicit topic validation.

## Failure Modes

- invalid topic or cursor: `400`
- too many topics: `400`
- oversized messages: `413`
- unavailable backend: `503`
- serialization or transport failure: application error

## Examples

- [`channels_sse.rs`](../vyuh/examples/channels/sse.rs): SSE subscription.
- [`channels_websocket.rs`](../vyuh/examples/channels/websocket.rs):
  WebSocket subscription.
- [`channels_long_poll.rs`](../vyuh/examples/channels/long_poll.rs): polling
  with cursors.
- [`channels_auth.rs`](../vyuh/examples/channels/auth.rs): topic selection
  after authentication.

## Current Limitations

- `LocalChannelBackend` is process-local and in-memory.
- Channels provide bounded replay, not durable delivery.
- Authorization is application-owned and belongs in route handlers.
- WebSocket client-side topic mutation is not part of the first API.

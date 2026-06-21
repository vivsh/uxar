use std::{convert::Infallible, time::Duration};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures::{SinkExt, StreamExt, stream};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

use crate::callables::{IntoReturnPart, ReturnPart, TypeSchema};

use super::{ChannelCursor, ChannelError, ChannelEvent, ChannelReceiver};

pub struct ChannelSse {
    replay: Vec<ChannelEvent>,
    receiver: ChannelReceiver,
    keepalive: Duration,
}

impl ChannelSse {
    pub(crate) fn new(
        replay: Vec<ChannelEvent>,
        receiver: ChannelReceiver,
        keepalive: Duration,
    ) -> Self {
        Self {
            replay,
            receiver,
            keepalive,
        }
    }
}

impl IntoResponse for ChannelSse {
    fn into_response(self) -> Response {
        let replay = stream::iter(self.replay.into_iter().map(event_to_sse));
        let live =
            ReceiverStream::new(self.receiver.inner).map(|event| event_to_sse((*event).clone()));
        let stream = replay.chain(live);
        Sse::new(stream)
            .keep_alive(KeepAlive::new().interval(self.keepalive))
            .into_response()
    }
}

impl IntoReturnPart for ChannelSse {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Unknown
    }
}

fn event_to_sse(event: ChannelEvent) -> Result<Event, Infallible> {
    let data = serde_json::to_string(&event.data).unwrap_or_else(|_| "null".to_string());
    Ok(Event::default()
        .id(event.id.to_string())
        .event(event.topic.as_str())
        .data(data))
}

pub struct ChannelWebSocket {
    upgrade: WebSocketUpgrade,
    replay: Vec<ChannelEvent>,
    receiver: ChannelReceiver,
}

impl ChannelWebSocket {
    pub(crate) fn new(
        upgrade: WebSocketUpgrade,
        replay: Vec<ChannelEvent>,
        receiver: ChannelReceiver,
    ) -> Self {
        Self {
            upgrade,
            replay,
            receiver,
        }
    }
}

impl IntoResponse for ChannelWebSocket {
    fn into_response(self) -> Response {
        self.upgrade
            .on_upgrade(|socket| websocket_task(socket, self.replay, self.receiver))
            .into_response()
    }
}

impl IntoReturnPart for ChannelWebSocket {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Unknown
    }
}

async fn websocket_task(
    socket: WebSocket,
    replay: Vec<ChannelEvent>,
    mut receiver: ChannelReceiver,
) {
    let (mut sender, mut incoming) = socket.split();
    let incoming_task = tokio::spawn(async move { while incoming.next().await.is_some() {} });

    for event in replay {
        if send_ws_event(&mut sender, &event).await.is_err() {
            incoming_task.abort();
            return;
        }
    }

    while let Some(event) = receiver.recv().await {
        if send_ws_event(&mut sender, event.as_ref()).await.is_err() {
            break;
        }
    }

    incoming_task.abort();
}

async fn send_ws_event(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    event: &ChannelEvent,
) -> Result<(), ChannelError> {
    let text =
        serde_json::to_string(event).map_err(|err| ChannelError::Serialization(err.to_string()))?;
    sender
        .send(Message::Text(text.into()))
        .await
        .map_err(|err| ChannelError::Transport(err.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ChannelLongPoll {
    pub cursor: Option<ChannelCursor>,
    pub events: Vec<ChannelEvent>,
}

impl ChannelLongPoll {
    pub(crate) fn from_events(events: Vec<ChannelEvent>) -> Self {
        let cursor = events.last().map(|event| ChannelCursor::new(event.id));
        Self { cursor, events }
    }
}

impl IntoResponse for ChannelLongPoll {
    fn into_response(self) -> Response {
        axum::Json(self).into_response()
    }
}

impl IntoReturnPart for ChannelLongPoll {
    fn into_return_part() -> ReturnPart {
        ReturnPart::Body(
            TypeSchema::wrap::<ChannelLongPoll>(),
            "application/json".into(),
        )
    }
}

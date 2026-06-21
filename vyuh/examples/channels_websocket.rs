//! WebSocket channel route.

use axum::extract::ws::WebSocketUpgrade;
use vyuh::{
    Error, bundles,
    channels::{ChannelRef, ChannelWebSocket},
};

#[bundles::route(path = "/events/ws", method = "GET")]
async fn events_ws(
    upgrade: WebSocketUpgrade,
    channels: ChannelRef,
) -> Result<ChannelWebSocket, Error> {
    channels
        .websocket(upgrade, "orders.updated")
        .await
        .map_err(Error::from)
}

fn main() {
    let bundle = bundles::bundle! {
        events_ws,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    println!("WebSocket channel route registered");
}

//! SSE channel route.

use vyuh::{
    Error, bundles,
    channels::{ChannelRef, ChannelSse},
};

#[bundles::route(path = "/events", method = "GET")]
async fn events(channels: ChannelRef) -> Result<ChannelSse, Error> {
    channels.sse("orders.updated").await.map_err(Error::from)
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        events,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    println!("SSE channel route registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



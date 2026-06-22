//! Long polling channel route.

use serde::Deserialize;
use vyuh::{
    Error, bundles,
    channels::{ChannelCursor, ChannelLongPoll, ChannelRef},
    routes::Query,
};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PollQuery {
    after: Option<ChannelCursor>,
}

#[bundles::route(path = "/events/poll", method = "GET")]
async fn events_poll(
    channels: ChannelRef,
    Query(query): Query<PollQuery>,
) -> Result<ChannelLongPoll, Error> {
    channels
        .long_poll("orders.updated", query.after)
        .await
        .map_err(Error::from)
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        events_poll,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    println!("long polling channel route registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



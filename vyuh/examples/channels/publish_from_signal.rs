//! Publish a signal payload to a channel.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, bundles, channels::ChannelRef};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct OrderUpdated {
    order_id: i64,
    status: String,
}

#[bundles::signal]
async fn publish_order_update(
    channels: ChannelRef,
    Data(event): Data<OrderUpdated>,
) -> Result<(), Error> {
    channels
        .publish(
            format!("orders.{}", event.order_id),
            Data::new(event.as_ref().clone()),
        )
        .await
        .map_err(Error::from)?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        publish_order_update,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    println!("signal-to-channel publisher registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



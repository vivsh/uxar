//! Application-owned channel topic authorization.

use vyuh::{
    Error,
    auth::AuthUser,
    bundles,
    channels::{ChannelRef, ChannelSse, ChannelTopic},
};

fn allowed_topics(user: &AuthUser) -> Result<Vec<ChannelTopic>, Error> {
    Ok(vec![
        ChannelTopic::new(format!("users.{}/orders", user.key)).map_err(Error::from)?,
        ChannelTopic::new("orders.public").map_err(Error::from)?,
    ])
}

#[bundles::route(path = "/account/events", method = "GET")]
async fn account_events(user: AuthUser, channels: ChannelRef) -> Result<ChannelSse, Error> {
    channels
        .sse(allowed_topics(&user)?)
        .await
        .map_err(Error::from)
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        account_events,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    println!("authenticated channel route registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



//! Exposing a trait facade from a concrete service.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example services_facade
//! ```

use std::sync::Arc;
use vyuh::{
    bundles,
    routes::Html,
    services::{Service, ServiceError, ServiceExposer, ServiceInstance, ServiceRef},
};

trait Greeter: Send + Sync {
    fn greet(&self, name: &str) -> String;
}

struct FriendlyGreeter;

impl Greeter for FriendlyGreeter {
    fn greet(&self, name: &str) -> String {
        format!("hello {name}")
    }
}

impl Service for FriendlyGreeter {
    fn expose(exposer: &mut ServiceExposer<Self>) -> Result<(), ServiceError> {
        exposer.expose(|service| service as Arc<dyn Greeter>)
    }
}

#[bundles::service]
async fn friendly_greeter() -> ServiceInstance<FriendlyGreeter> {
    FriendlyGreeter.into()
}

#[bundles::route(path = "/hello")]
async fn hello(greeter: ServiceRef<dyn Greeter>) -> Html<String> {
    Html(greeter.greet("Vyuh"))
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        friendly_greeter,
        hello,
    };

    assert_eq!(bundle.reverse("hello", &[]), Some("/hello".to_string()));
    println!("service facade registered");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



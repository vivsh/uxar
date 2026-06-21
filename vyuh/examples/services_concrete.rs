//! Concrete service registration and route extraction.
//!
//! Run:
//!
//! ```sh
//! cargo run --example services_basic
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use vyuh::{
    bundles,
    routes::Html,
    services::{Service, ServiceInstance, ServiceRef},
};

#[derive(Default)]
struct Counter {
    value: AtomicUsize,
}

impl Counter {
    fn next(&self) -> usize {
        self.value.fetch_add(1, Ordering::SeqCst) + 1
    }
}

impl Service for Counter {}

#[bundles::service]
async fn counter() -> ServiceInstance<Counter> {
    Counter::default().into()
}

#[bundles::route(path = "/count")]
async fn count(counter: ServiceRef<Counter>) -> Html<String> {
    Html(counter.next().to_string())
}

fn main() {
    let bundle = bundles::bundle! {
        counter,
        count,
    };

    assert_eq!(bundle.reverse("count", &[]), Some("/count".to_string()));
    println!("basic service registered");
}

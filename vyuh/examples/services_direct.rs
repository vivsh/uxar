//! Direct service registration without the service macro.
//!
//! Run:
//!
//! ```sh
//! cargo run --example services_direct
//! ```

use vyuh::{
    bundles,
    services::{Service, ServiceInstance},
};

struct AppClock;

impl AppClock {
    fn label(&self) -> &'static str {
        "clock"
    }
}

impl Service for AppClock {}

async fn app_clock() -> ServiceInstance<AppClock> {
    AppClock.into()
}

fn main() {
    let bundle = bundles::bundle([bundles::service(app_clock)]);

    assert_eq!(bundle.iter_operations().count(), 1);
    let _method: fn(&AppClock) -> &'static str = AppClock::label;
    println!("direct service registered");
}

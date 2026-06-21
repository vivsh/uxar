//! Service-owned background worker with shutdown handling.
//!
//! Run:
//!
//! ```sh
//! cargo run --example services_worker
//! ```

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use vyuh::{
    Site, bundles,
    services::{Service, ServiceError, ServiceInstance, ServiceRunner},
};

struct HeartbeatService {
    beats: Arc<AtomicUsize>,
}

impl HeartbeatService {
    fn beats(&self) -> usize {
        self.beats.load(Ordering::SeqCst)
    }
}

impl Service for HeartbeatService {
    fn run(&mut self, runner: &mut ServiceRunner) -> Result<(), ServiceError> {
        let beats = self.beats.clone();
        runner.run("heartbeat", move |site: Site| {
            let beats = beats.clone();
            async move {
                let shutdown = site.shutdown_notifier();
                loop {
                    tokio::select! {
                        _ = shutdown.notified() => break,
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {
                            beats.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
                Ok(())
            }
        })
    }
}

#[bundles::service]
async fn heartbeat_service() -> ServiceInstance<HeartbeatService> {
    HeartbeatService {
        beats: Arc::new(AtomicUsize::new(0)),
    }
    .into()
}

fn main() {
    let bundle = bundles::bundle! {
        heartbeat_service,
    };

    assert_eq!(bundle.iter_operations().count(), 1);
    let _method: fn(&HeartbeatService) -> usize = HeartbeatService::beats;
    println!("service worker registered");
}

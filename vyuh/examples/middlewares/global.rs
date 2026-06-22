//! Basic site-wide HTTP middleware configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example middlewares_global
//! ```

use vyuh::{
    SiteConf,
    middlewares::{BodyLimitConf, CompressionConf, HttpConf, TraceConf},
};

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().http(HttpConf {
        trace: TraceConf { enabled: true },
        compression: CompressionConf { enabled: true },
        body_limit: BodyLimitConf {
            enabled: true,
            max_bytes: 1024 * 1024,
        },
        ..HttpConf::default()
    });

    assert!(conf.http.trace.enabled);
    assert!(conf.http.compression.enabled);
    assert_eq!(conf.http.body_limit.max_bytes, 1024 * 1024);
    println!("configured site-wide HTTP middleware");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



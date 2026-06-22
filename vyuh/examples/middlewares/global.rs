//! Basic site-wide HTTP middleware configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example middlewares_global
//! ```

use vyuh::{
    SiteConf,
    middlewares::{BodyLimitConf, CompressionConf, HttpConf, TraceConf},
};

fn main() {
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
}

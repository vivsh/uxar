//! Logging configuration with stdout and rotating file sinks.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example logging_setup
//! ```
//!
//! Override filters with:
//!
//! ```sh
//! VYUH_LOG=info cargo run -p vyuh --no-default-features --features sqlite --example logging_setup
//! VYUH_LOG_AUDIT=off cargo run -p vyuh --no-default-features --features sqlite --example logging_setup
//! ```

use vyuh::{
    SiteConf,
    logging::{LogRule, LogSink, LoggingConf, Rotation},
};

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().project_dir(".").logging(LoggingConf {
        env_prefix: Some("VYUH_LOG".into()),
        rules: vec![
            LogRule {
                name: "APP".into(),
                sink: LogSink::Stdout { pretty: true },
                default_filter: "info,vyuh=warn".into(),
            },
            LogRule {
                name: "AUDIT".into(),
                sink: LogSink::File {
                    dir: "target/vyuh-example-logs".into(),
                    rotation: Rotation::Daily,
                },
                default_filter: "warn".into(),
            },
        ],
    });

    assert_eq!(conf.logging.resolved_env_prefix(), "VYUH_LOG");
    conf.logging.validate().unwrap();

    println!("logging configured; build a Site with this SiteConf to initialize tracing");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



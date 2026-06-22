//! Template date/time formatting configuration and Rust utilities.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example templates_datetime
//! ```

use vyuh::{
    Site, SiteConf,
    templates::{TemplateConf, TemplateDateFormats, TemplateFormatError},
};

fn published_label(
    site: &Site,
    published_at: chrono::DateTime<chrono::Utc>,
) -> Result<String, TemplateFormatError> {
    vyuh::templates::format_datetime(site, published_at, None)
}

fn published_day(
    site: &Site,
    published_at: chrono::DateTime<chrono::Utc>,
) -> Result<String, TemplateFormatError> {
    vyuh::templates::format_date(site, published_at, Some("%d %b %Y"))
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default()
        .timezone("Asia/Kolkata")
        .templates(TemplateConf {
            date_formats: TemplateDateFormats {
                date: "%d %b %Y".into(),
                time: "%H:%M".into(),
                datetime: "%d %b %Y, %H:%M".into(),
            },
            ..TemplateConf::default()
        });

    let _ = published_label;
    let _ = published_day;
    assert_eq!(conf.templates.date_formats.datetime, "%d %b %Y, %H:%M");
    println!("configured template date/time formatting");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



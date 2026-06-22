//! Template environment configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example templates_config
//! ```

use vyuh::{
    SiteConf,
    templates::{TemplateAutoEscape, TemplateConf, TemplateDateFormats, TemplateUndefined},
};

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().templates(TemplateConf {
        dirs: vec!["templates".into()],
        auto_escape: TemplateAutoEscape::ByExtension,
        undefined: TemplateUndefined::Strict,
        trim_blocks: true,
        lstrip_blocks: true,
        keep_trailing_newline: true,
        date_formats: TemplateDateFormats {
            date: "%d %b %Y".into(),
            time: "%H:%M".into(),
            datetime: "%d %b %Y, %H:%M".into(),
        },
    });

    assert_eq!(conf.templates.dirs, vec!["templates"]);
    assert!(conf.templates.trim_blocks);
    assert_eq!(conf.templates.date_formats.date, "%d %b %Y");
    println!("configured template environment");
    let bundle = vyuh::bundles::Bundle::new();
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



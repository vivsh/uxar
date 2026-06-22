//! Template environment configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example templates_config
//! ```

use vyuh::{
    SiteConf,
    templates::{TemplateAutoEscape, TemplateConf, TemplateDateFormats, TemplateUndefined},
};

fn main() {
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
}

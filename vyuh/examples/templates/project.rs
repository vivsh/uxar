//! Basic project template directory configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example templates_project
//! ```

use vyuh::{
    SiteConf, bundles,
    routes::Html,
    templates::{TemplateError, Templates},
};

#[bundles::route(path = "/hello")]
async fn hello(templates: Templates) -> Result<Html<String>, TemplateError> {
    templates.html("hello.html", &serde_json::json!({ "name": "Vyuh" }))
}

fn main() {
    let conf = SiteConf::default().templates_dir("templates");
    let bundle = bundles::bundle! {
        hello,
    };

    assert_eq!(conf.templates.dirs, vec!["templates"]);
    assert!(bundle.reverse("hello", &[]).is_some());
    println!("configured project templates from ./templates");
}

//! Basic project template directory configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example templates_project
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

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default().templates_dir("templates");
    let bundle = bundles::bundle! {
        hello,
    };

    assert_eq!(conf.templates.dirs, vec!["templates"]);
    assert!(bundle.reverse("hello", &[]).is_some());
    println!("configured project templates from ./templates");
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



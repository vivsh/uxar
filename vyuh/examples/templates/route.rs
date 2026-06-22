//! Route handler extraction of the Templates handle.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example templates_route
//! ```

use vyuh::{
    bundles,
    routes::Html,
    templates::{TemplateError, Templates},
};

#[bundles::route(path = "/dashboard")]
async fn dashboard(templates: Templates) -> Result<Html<String>, TemplateError> {
    templates.html(
        "dashboard.html",
        &serde_json::json!({
            "title": "Dashboard",
        }),
    )
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        dashboard,
    };

    assert_eq!(
        bundle.reverse("dashboard", &[]),
        Some("/dashboard".to_string())
    );
    println!("Templates can be extracted directly by route handlers");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



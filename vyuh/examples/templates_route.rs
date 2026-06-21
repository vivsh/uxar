//! Route handler extraction of the Templates handle.
//!
//! Run:
//!
//! ```sh
//! cargo run --example templates_route
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

fn main() {
    let bundle = bundles::bundle! {
        dashboard,
    };

    assert_eq!(
        bundle.reverse("dashboard", &[]),
        Some("/dashboard".to_string())
    );
    println!("Templates can be extracted directly by route handlers");
}

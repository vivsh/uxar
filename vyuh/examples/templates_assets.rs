//! Templates shipped inside a bundle asset directory.
//!
//! Run:
//!
//! ```sh
//! cargo run --example templates_assets
//! ```

use rust_silos::{Silo, embed_silo};
use vyuh::{
    bundles, embed,
    routes::Html,
    templates::{TemplateError, Templates},
};

const EXAMPLE_ASSETS: Silo = embed_silo!("examples/templates_assets_assets");

#[bundles::asset_dir]
fn example_assets() -> embed::Dir {
    embed::Dir::new(EXAMPLE_ASSETS.clone())
}

#[bundles::route(path = "/asset-template")]
async fn asset_template(templates: Templates) -> Result<Html<String>, TemplateError> {
    templates.html(
        "examples/asset.html",
        &serde_json::json!({ "source": "asset dir" }),
    )
}

fn main() {
    let bundle = bundles::bundle! {
        example_assets,
        asset_template,
    };

    assert!(bundle.reverse("asset_template", &[]).is_some());
    println!("registered template from bundle asset dir");
}

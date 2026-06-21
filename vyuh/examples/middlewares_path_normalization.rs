//! Slash policy configuration at site, bundle, and route levels.
//!
//! Run:
//!
//! ```sh
//! cargo run --example middlewares_slashes
//! ```

use vyuh::{
    SiteConf, bundles,
    middlewares::{HttpConf, SlashConf, SlashPolicy},
    routes::Html,
};

#[bundles::route(path = "/docs/", slash = "redirect_append")]
async fn docs() -> Html<String> {
    Html("docs".to_string())
}

fn main() {
    let conf = SiteConf::default().http(HttpConf {
        slash: SlashConf {
            policy: SlashPolicy::Auto,
        },
        ..HttpConf::default()
    });

    let bundle = bundles::bundle! {
        docs,
    }
    .with_slash_policy(SlashPolicy::Auto);

    assert_eq!(conf.http.slash.policy, SlashPolicy::Auto);
    assert_eq!(bundle.reverse("docs", &[]), Some("/docs/".to_string()));
    println!("configured slash policy");
}

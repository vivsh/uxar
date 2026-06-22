//! Verifier-backed API key route.
//!
//! Run:
//!
//! ```sh
//! cargo run --example auth_api_key
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    SiteConf,
    auth::{ApiKey, ApiKeyConf, ApiKeyPrincipal, ApiKeyVerifier, AuthConf, AuthError},
    bundles,
    routes::Json,
};

struct ExampleVerifier;

impl ApiKeyVerifier for ExampleVerifier {
    async fn verify(&self, presented: &str) -> Result<ApiKeyPrincipal, AuthError> {
        if presented == "secret" {
            Ok(ApiKeyPrincipal::new("key-1").subject("service-1"))
        } else {
            Err(AuthError::InvalidApiKey)
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct KeyInfo {
    key_id: String,
}

#[bundles::route(path = "/ingest", method = "POST")]
async fn ingest(key: ApiKey) -> Json<KeyInfo> {
    Json(KeyInfo {
        key_id: key.key_id.to_string(),
    })
}

fn main() {
    let conf = SiteConf::default()
        .auth(AuthConf::default().api_keys(ApiKeyConf::default().verifier(ExampleVerifier)));
    let bundle = bundles::bundle! {
        ingest,
    };

    assert!(conf.auth.api_keys.enabled);
    assert_eq!(bundle.reverse("ingest", &[]), Some("/ingest".to_string()));
    println!("api key auth route registered");
}

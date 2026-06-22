//! API key OpenAPI security metadata.
//!
//! Run:
//!
//! ```sh
//! cargo run --example auth_api_key_openapi
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    auth::{ApiKey, ApiKeyPrincipal, ApiKeyVerifier, AuthError},
    bundles,
    routes::Json,
};

struct ExampleVerifier;

impl ApiKeyVerifier for ExampleVerifier {
    async fn verify(&self, presented: &str) -> Result<ApiKeyPrincipal, AuthError> {
        if presented == "secret" {
            Ok(ApiKeyPrincipal::new("key-1"))
        } else {
            Err(AuthError::InvalidApiKey)
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct Accepted {
    ok: bool,
}

#[bundles::route(path = "/events", method = "POST")]
async fn events(_key: ApiKey) -> Json<Accepted> {
    Json(Accepted { ok: true })
}

fn main() {
    let _verifier = ExampleVerifier;
    let bundle = bundles::bundle! {
        events,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("API Key API")
            .spec("/openapi.json"),
    );

    assert_eq!(bundle.reverse("events", &[]), Some("/events".to_string()));
    println!("api key OpenAPI security metadata registered");
}

//! JWT token creation and AuthUser route extraction.
//!
//! Run:
//!
//! ```sh
//! cargo run --example auth_basic
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    Site, SiteConf,
    auth::{AuthError, AuthUser},
    bundles,
    routes::Json,
};

#[derive(Debug, Serialize, JsonSchema)]
struct LoginResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Profile {
    key: String,
    roles: u64,
}

#[bundles::route(path = "/login", method = "POST")]
async fn login(site: Site) -> Result<Json<LoginResponse>, AuthError> {
    let user = AuthUser::new("user-123", 0);
    let tokens = site.auth().create_token_pair(user, &["web"])?;
    Ok(Json(LoginResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
    }))
}

#[bundles::route(path = "/me")]
async fn me(user: AuthUser) -> Json<Profile> {
    Json(Profile {
        key: user.key.to_string(),
        roles: user.roles,
    })
}

fn main() {
    let conf = SiteConf::default().secret_key("auth-basic-example-secret");
    let bundle = bundles::bundle! {
        login,
        me,
    };

    assert_eq!(bundle.reverse("login", &[]), Some("/login".to_string()));
    assert_eq!(bundle.reverse("me", &[]), Some("/me".to_string()));
    println!(
        "auth routes registered with secret length {}",
        conf.secret_key.len()
    );
}

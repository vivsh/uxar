//! Opt-in JWT cookies and refresh token flow.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example auth_cookies
//! ```

use vyuh::{
    SiteConf,
    auth::{AuthConf, AuthUser, CookieConf, CookieSameSite},
    bundles,
};

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default()
        .secret_key("auth-cookie-example-secret-minimum-32-chars")
        .auth(
            AuthConf::default()
                .access_cookie(CookieConf::new("access_token"))
                .refresh_cookie(CookieConf::new("refresh_token").same_site(CookieSameSite::Strict)),
        );

    let user = AuthUser::new("user-123", 0);
    let bundle = bundles::Bundle::new();

    assert_eq!(user.key.as_ref(), "user-123");
    assert!(conf.auth.access_cookie.is_some());
    assert!(conf.auth.refresh_cookie.is_some());
    assert!(bundle.iter_operations().next().is_none());
    println!("cookie auth configured explicitly");
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;




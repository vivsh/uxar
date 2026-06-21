use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    SiteConf,
    auth::{AuthUser, BitRole, permit},
    bundles,
    routes::{Json, StatusCode},
    testing::TestClient,
};

fn test_conf() -> SiteConf {
    SiteConf {
        secret_key: "auth-test-secret-minimum-32-chars".to_string(),
        log_init: false,
        logging: vyuh::logging::LoggingConf {
            env_prefix: None,
            rules: vec![],
        },
        ..SiteConf::default()
    }
}

#[derive(BitRole)]
enum TestRole {
    Manager,
    Viewer,
}

#[derive(Debug, Serialize, JsonSchema)]
struct WhoAmI {
    key: String,
    roles: u64,
}

#[bundles::route(path = "/me")]
async fn me(user: AuthUser) -> Json<WhoAmI> {
    Json(WhoAmI {
        key: user.key.to_string(),
        roles: user.roles,
    })
}

#[bundles::route(path = "/secure")]
async fn secure(_permit: permit!(TestRole, Manager)) -> Json<WhoAmI> {
    Json(WhoAmI {
        key: "manager".to_string(),
        roles: TestRole::Manager.to_role_type(),
    })
}

#[tokio::test]
async fn auth_accepts_bearer_authorization_header() {
    let site = vyuh::build_site(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .authenticator()
        .create_token_pair(
            AuthUser::new("user-1", TestRole::Viewer.to_role_type()),
            &[],
        )
        .unwrap()
        .access_token;
    let client = TestClient::new(site.clone());

    client
        .get("/me")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
        .assert_status(StatusCode::OK);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn auth_accepts_legacy_jwt_authorization_header() {
    let site = vyuh::build_site(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .authenticator()
        .create_token_pair(
            AuthUser::new("user-1", TestRole::Viewer.to_role_type()),
            &[],
        )
        .unwrap()
        .access_token;
    let client = TestClient::new(site.clone());

    client
        .get("/me")
        .header("authorization", &format!("JWT {token}"))
        .send()
        .await
        .assert_status(StatusCode::OK);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn auth_missing_token_returns_unauthorized() {
    let site = vyuh::build_site(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    client
        .get("/me")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn auth_permit_rejects_missing_role() {
    let site = vyuh::build_site(
        test_conf(),
        bundles::bundle! {
            secure,
        },
    )
    .await
    .unwrap();
    let token = site
        .authenticator()
        .create_token_pair(
            AuthUser::new("user-1", TestRole::Viewer.to_role_type()),
            &[],
        )
        .unwrap()
        .access_token;
    let client = TestClient::new(site.clone());

    client
        .get("/secure")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN);

    site.shutdown_and_wait().await;
}

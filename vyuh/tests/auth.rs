use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    SiteConf,
    auth::{
        ApiKey, ApiKeyConf, ApiKeyPrincipal, ApiKeyVerifier, AuthAudiencePolicy, AuthConf,
        AuthError, AuthUser, BitRole, JWTClaim, TokenKind, permit,
    },
    bundles, routes,
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

#[derive(Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
struct KeyInfo {
    key_id: String,
    subject: Option<String>,
    roles: u64,
}

struct StaticApiKeyVerifier;

impl ApiKeyVerifier for StaticApiKeyVerifier {
    async fn verify(&self, presented: &str) -> Result<ApiKeyPrincipal, AuthError> {
        if presented == "valid-key" {
            Ok(ApiKeyPrincipal::new("key-1")
                .subject("service-1")
                .roles(TestRole::Viewer.to_role_type()))
        } else {
            Err(AuthError::InvalidApiKey)
        }
    }
}

#[bundles::route(path = "/public")]
async fn public() -> Json<&'static str> {
    Json("ok")
}

#[bundles::route(path = "/me")]
async fn me(user: AuthUser) -> Json<WhoAmI> {
    Json(WhoAmI {
        key: user.key.to_string(),
        roles: user.roles,
    })
}

#[bundles::route(path = "/api-key")]
async fn api_key_route(key: ApiKey) -> Json<KeyInfo> {
    Json(KeyInfo {
        key_id: key.key_id.to_string(),
        subject: key.subject.as_ref().map(ToString::to_string),
        roles: key.roles,
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
    let site = vyuh::Site::build(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .auth()
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
async fn public_route_does_not_require_auth() {
    let site = vyuh::Site::build(
        test_conf(),
        bundles::bundle! {
            public,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    client
        .get("/public")
        .send()
        .await
        .assert_status(StatusCode::OK);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn auth_accepts_legacy_jwt_authorization_header() {
    let site = vyuh::Site::build(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .auth()
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
    let site = vyuh::Site::build(
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
    let site = vyuh::Site::build(
        test_conf(),
        bundles::bundle! {
            secure,
        },
    )
    .await
    .unwrap();
    let token = site
        .auth()
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

#[tokio::test]
async fn auth_user_rejects_refresh_token() {
    let site = vyuh::Site::build(
        test_conf(),
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .auth()
        .create_token_pair(
            AuthUser::new("user-1", TestRole::Viewer.to_role_type()),
            &[],
        )
        .unwrap()
        .refresh_token;
    let client = TestClient::new(site.clone());

    client
        .get("/me")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn refresh_rejects_access_token() {
    let site = vyuh::Site::build(test_conf(), bundles::Bundle::new())
        .await
        .unwrap();
    let token = site
        .auth()
        .create_token_pair(
            AuthUser::new("user-1", TestRole::Viewer.to_role_type()),
            &[],
        )
        .unwrap()
        .access_token;
    let req = routes::Request::builder()
        .header("authorization", format!("Bearer {token}"))
        .body(routes::Body::empty())
        .unwrap();
    let (parts, _) = req.into_parts();

    let err = site.auth().refresh(&parts, &[]).unwrap_err();
    assert!(matches!(err, AuthError::WrongTokenKind));

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn audience_required_rejects_route_without_audience() {
    let conf = test_conf().auth(AuthConf::default().audience(AuthAudiencePolicy::Required));
    let site = vyuh::Site::build(
        conf,
        bundles::bundle! {
            me,
        },
    )
    .await
    .unwrap();
    let token = site
        .auth()
        .create_token_pair(AuthUser::new("user-1", 0), &["web"])
        .unwrap()
        .access_token;
    let client = TestClient::new(site.clone());

    client
        .get("/me")
        .header("authorization", &format!("Bearer {token}"))
        .send()
        .await
        .assert_status(StatusCode::FORBIDDEN);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn issuer_must_match_when_configured() {
    let conf = test_conf().auth(AuthConf::default().issuer("expected"));
    let site = vyuh::Site::build(conf, bundles::Bundle::new())
        .await
        .unwrap();
    let claims = JWTClaim::new(
        &AuthUser::new("user-1", 0),
        "",
        Some("wrong".to_string()),
        vec![],
        3600,
        TokenKind::Access,
    );
    let token = site.auth().encode(&claims).unwrap();

    let err = site.auth().decode(&token).unwrap_err();
    assert!(matches!(
        err,
        AuthError::InvalidToken | AuthError::InternalError(_)
    ));

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn leeway_allows_recently_expired_tokens() {
    let conf = test_conf().auth(AuthConf::default().leeway_seconds(30));
    let site = vyuh::Site::build(conf, bundles::Bundle::new())
        .await
        .unwrap();
    let claims = JWTClaim::new(
        &AuthUser::new("user-1", 0),
        "",
        None,
        vec![],
        -10,
        TokenKind::Access,
    );
    let token = site.auth().encode(&claims).unwrap();

    site.auth().decode(&token).unwrap();

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn configured_minimum_secret_length_is_validated() {
    let err = vyuh::Site::build(
        SiteConf::default()
            .secret_key("short")
            .log_init(false)
            .auth(AuthConf::default().min_secret_len(32)),
        bundles::Bundle::new(),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("secret_key"));
}

#[tokio::test]
async fn default_cookies_are_disabled() {
    let site = vyuh::Site::build(test_conf(), bundles::Bundle::new())
        .await
        .unwrap();
    let mut response = routes::Response::new(routes::Body::empty());
    site.auth()
        .login_user(AuthUser::new("user-1", 0), &[], &mut response)
        .unwrap();

    assert!(
        response
            .headers()
            .get_all("set-cookie")
            .iter()
            .next()
            .is_none()
    );

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn opt_in_cookies_are_written() {
    let conf = test_conf().auth(AuthConf::cookie_pair("access_token", "refresh_token"));
    let site = vyuh::Site::build(conf, bundles::Bundle::new())
        .await
        .unwrap();
    let mut response = routes::Response::new(routes::Body::empty());
    site.auth()
        .login_user(AuthUser::new("user-1", 0), &[], &mut response)
        .unwrap();

    assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn api_key_extracts_from_configured_header() {
    let conf = test_conf()
        .auth(AuthConf::default().api_keys(ApiKeyConf::default().verifier(StaticApiKeyVerifier)));
    let site = vyuh::Site::build(
        conf,
        bundles::bundle! {
            api_key_route,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    client
        .get("/api-key")
        .header("x-api-key", "valid-key")
        .send()
        .await
        .assert_json(
            StatusCode::OK,
            &KeyInfo {
                key_id: "key-1".to_string(),
                subject: Some("service-1".to_string()),
                roles: TestRole::Viewer.to_role_type(),
            },
        )
        .await;

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn api_key_authorization_scheme_works_when_configured() {
    let conf = test_conf()
        .auth(AuthConf::default().api_keys(ApiKeyConf::default().verifier(StaticApiKeyVerifier)));
    let site = vyuh::Site::build(
        conf,
        bundles::bundle! {
            api_key_route,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    client
        .get("/api-key")
        .header("authorization", "ApiKey valid-key")
        .send()
        .await
        .assert_status(StatusCode::OK);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn api_key_query_param_is_explicit_opt_in() {
    let disabled_conf = test_conf()
        .auth(AuthConf::default().api_keys(ApiKeyConf::default().verifier(StaticApiKeyVerifier)));
    let disabled_site = vyuh::Site::build(
        disabled_conf,
        bundles::bundle! {
            api_key_route,
        },
    )
    .await
    .unwrap();
    let disabled_client = TestClient::new(disabled_site.clone());

    disabled_client
        .get("/api-key?api_key=valid-key")
        .send()
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    disabled_site.shutdown_and_wait().await;

    let enabled_conf = test_conf().auth(
        AuthConf::default().api_keys(
            ApiKeyConf::default()
                .allow_query_param(true)
                .verifier(StaticApiKeyVerifier),
        ),
    );
    let enabled_site = vyuh::Site::build(
        enabled_conf,
        bundles::bundle! {
            api_key_route,
        },
    )
    .await
    .unwrap();
    let enabled_client = TestClient::new(enabled_site.clone());

    enabled_client
        .get("/api-key?api_key=valid-key")
        .send()
        .await
        .assert_status(StatusCode::OK);

    enabled_site.shutdown_and_wait().await;
}

#[tokio::test]
async fn api_key_missing_verifier_returns_server_error() {
    let conf = test_conf().auth(AuthConf::default().api_keys(ApiKeyConf::default().enabled(true)));
    let site = vyuh::Site::build(
        conf,
        bundles::bundle! {
            api_key_route,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    client
        .get("/api-key")
        .header("x-api-key", "valid-key")
        .send()
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn api_key_openapi_security_scheme_is_generated() {
    let conf = test_conf()
        .auth(AuthConf::default().api_keys(ApiKeyConf::default().verifier(StaticApiKeyVerifier)));
    let bundle = bundles::bundle! {
        api_key_route,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Auth API")
            .spec("/openapi.json"),
    );
    let site = vyuh::Site::build(conf, bundle).await.unwrap();
    let client = TestClient::new(site.clone());

    let spec: serde_json::Value = client
        .get("/openapi.json")
        .send()
        .await
        .assert_ok()
        .json()
        .await;
    assert_eq!(
        spec["components"]["securitySchemes"]["apiKeyAuth"]["type"],
        "apiKey"
    );
    assert_eq!(
        spec["components"]["securitySchemes"]["apiKeyAuth"]["name"],
        "X-API-Key"
    );

    site.shutdown_and_wait().await;
}

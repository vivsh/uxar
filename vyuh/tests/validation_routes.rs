use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vyuh::routes::IntoResponse;
use vyuh::{
    SiteConf, Validate, bundles,
    errors::ErrorConf,
    routes::{Json, Path, Query, StatusCode, Valid},
    testing::TestClient,
};

fn test_conf() -> SiteConf {
    SiteConf {
        log_init: false,
        logging: vyuh::logging::LoggingConf {
            env_prefix: None,
            rules: vec![],
        },
        ..SiteConf::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
    #[validate(min_length = 3)]
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct SearchUsers {
    #[validate(min_length = 2)]
    q: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct UserPath {
    #[validate(min = 1)]
    id: i64,
}

#[bundles::route(path = "/parse", method = "POST")]
async fn parse_only(Json(input): Json<CreateUser>) -> Json<CreateUser> {
    Json(input)
}

#[bundles::route(path = "/valid", method = "POST")]
async fn valid_json(Valid(Json(input)): Valid<Json<CreateUser>>) -> Json<CreateUser> {
    Json(input)
}

#[bundles::route(path = "/search")]
async fn valid_query(Valid(Query(input)): Valid<Query<SearchUsers>>) -> Json<SearchUsers> {
    Json(input)
}

#[bundles::route(path = "/users/{id}")]
async fn valid_path(Valid(Path(input)): Valid<Path<UserPath>>) -> Json<UserPath> {
    Json(input)
}

async fn validation_site(openapi: bool) -> vyuh::Site {
    let bundle = bundles::bundle! {
        parse_only,
        valid_json,
        valid_query,
        valid_path,
    };
    let bundle = if openapi {
        bundle.with_openapi(
            bundles::OpenApiConf::default()
                .title("Validation API")
                .version("0.1.0")
                .spec("/openapi.json"),
        )
    } else {
        bundle
    };
    vyuh::build_site(test_conf(), bundle).await.unwrap()
}

#[tokio::test]
async fn plain_json_does_not_validate() {
    let site = validation_site(false).await;
    let client = TestClient::new(site.clone());

    client
        .post("/parse")
        .json(&serde_json::json!({
            "email": "not-an-email",
            "name": "x"
        }))
        .send()
        .await
        .assert_status(StatusCode::OK);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn valid_json_returns_structured_422() {
    let site = validation_site(false).await;
    let client = TestClient::new(site.clone());

    let body: Value = client
        .post("/valid")
        .json(&serde_json::json!({
            "email": "not-an-email",
            "name": "x"
        }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .json()
        .await;

    assert_eq!(body["source"], "validation");
    assert_eq!(body["code"], "validation_error");
    assert!(body["errors"]["email"].is_array());
    assert!(body["errors"]["name"].is_array());

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn valid_query_and_path_share_error_shape() {
    let site = validation_site(false).await;
    let client = TestClient::new(site.clone());

    let query_body: Value = client
        .get("/search?q=x")
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .json()
        .await;
    assert_eq!(query_body["source"], "validation");
    assert!(query_body["errors"]["q"].is_array());

    let path_body: Value = client
        .get("/users/0")
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY)
        .json()
        .await;
    assert_eq!(path_body["source"], "validation");
    assert!(path_body["errors"]["id"].is_array());

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn parse_errors_return_400_error_report() {
    let site = validation_site(false).await;
    let client = TestClient::new(site.clone());

    let body: Value = client
        .post("/parse")
        .header("content-type", "application/json")
        .body(axum::body::Body::from("{bad json"))
        .send()
        .await
        .assert_status(StatusCode::BAD_REQUEST)
        .json()
        .await;

    assert_eq!(body["source"], "parse");
    assert_eq!(body["code"], "bad_request");

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn openapi_constraints_appear_only_for_valid_inputs() {
    let site = validation_site(true).await;
    let client = TestClient::new(site.clone());

    let spec: Value = client
        .get("/openapi.json")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;

    let components = &spec["components"]["schemas"];
    let create_user = components
        .as_object()
        .and_then(|schemas| schemas.get("CreateUser"))
        .expect("CreateUser schema should exist");
    let plain_props = &create_user["properties"];
    assert!(plain_props["email"].get("format").is_none());
    assert!(plain_props["name"].get("minLength").is_none());

    let parse_body =
        &spec["paths"]["/parse"]["post"]["requestBody"]["content"]["application/json"]["schema"];
    let valid_body =
        &spec["paths"]["/valid"]["post"]["requestBody"]["content"]["application/json"]["schema"];

    assert_eq!(parse_body["$ref"], "#/components/schemas/CreateUser");
    assert!(valid_body.get("$ref").is_none());
    assert_eq!(valid_body["properties"]["email"]["format"], "email");
    assert_eq!(valid_body["properties"]["name"]["minLength"], 3);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn custom_error_handler_can_replace_response() {
    let conf = test_conf().errors(ErrorConf::default().handler(|ctx, report| async move {
        assert_eq!(ctx.path, "/valid");
        let body = format!("{:?}:{}", report.source, report.code);
        (
            StatusCode::IM_A_TEAPOT,
            [("content-type", "text/plain")],
            body,
        )
            .into_response()
    }));
    let site = vyuh::build_site(
        conf,
        bundles::bundle! {
            valid_json,
        },
    )
    .await
    .unwrap();
    let client = TestClient::new(site.clone());

    let text = client
        .post("/valid")
        .json(&serde_json::json!({
            "email": "not-an-email",
            "name": "x"
        }))
        .send()
        .await
        .assert_status(StatusCode::IM_A_TEAPOT)
        .text()
        .await;

    assert!(text.ends_with(":validation_error"));
    site.shutdown_and_wait().await;
}

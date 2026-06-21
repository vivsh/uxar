use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vyuh::{
    SiteConf, Validate, bundles,
    routes::{BodyBytes, Json, Path, Query, StatusCode, Valid},
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
struct CreateNote {
    #[validate(min_length = 3)]
    title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct SearchParams {
    q: String,
    page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UserPath {
    id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct NoteOut {
    id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UserInOrg {
    org: String,
    id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct WebhookResult {
    len: usize,
}

#[bundles::route(path = "/notes", method = "POST")]
async fn create_note(Json(input): Json<CreateNote>) -> Json<CreateNote> {
    Json(input)
}

#[bundles::route(path = "/validated-notes", method = "POST")]
async fn create_valid_note(Valid(Json(input)): Valid<Json<CreateNote>>) -> Json<CreateNote> {
    Json(input)
}

#[bundles::route(path = "/search")]
async fn search(Query(params): Query<SearchParams>) -> Json<SearchParams> {
    Json(params)
}

#[bundles::route(path = "/users/{id}")]
async fn user_detail(Path(path): Path<UserPath>) -> Json<NoteOut> {
    Json(NoteOut { id: path.id })
}

#[bundles::route(path = "/orgs/{org}/users/{id}")]
async fn user_in_org(Path((org, id)): Path<(String, u64)>) -> Json<UserInOrg> {
    Json(UserInOrg { org, id })
}

#[bundles::route(path = "/webhook", method = "POST")]
async fn webhook(BodyBytes(bytes): BodyBytes) -> Json<WebhookResult> {
    Json(WebhookResult { len: bytes.len() })
}

async fn request_data_site(openapi: bool) -> vyuh::Site {
    let bundle = bundles::bundle! {
        create_note,
        create_valid_note,
        search,
        user_detail,
        user_in_org,
        webhook,
    };
    let bundle = if openapi {
        bundle.with_openapi(
            bundles::OpenApiConf::default()
                .title("Request Data")
                .version("0.1.0")
                .spec("/openapi.json"),
        )
    } else {
        bundle
    };
    vyuh::build_site(test_conf(), bundle).await.unwrap()
}

#[tokio::test]
async fn request_data_documentation_signatures_work() {
    let site = request_data_site(false).await;
    let client = TestClient::new(site.clone());

    let tuple_path: Value = client
        .get("/orgs/acme/users/42")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;
    assert_eq!(tuple_path["org"], "acme");
    assert_eq!(tuple_path["id"], 42);

    let webhook: Value = client
        .post("/webhook")
        .body(axum::body::Body::from("signed-payload"))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;
    assert_eq!(webhook["len"], 14);

    let parse_only: Value = client
        .post("/notes")
        .json(&serde_json::json!({ "title": "x" }))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;
    assert_eq!(parse_only["title"], "x");

    client
        .post("/validated-notes")
        .json(&serde_json::json!({ "title": "x" }))
        .send()
        .await
        .assert_status(StatusCode::UNPROCESSABLE_ENTITY);

    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn body_bytes_is_documented_as_binary_openapi_body() {
    let site = request_data_site(true).await;
    let client = TestClient::new(site.clone());

    let spec: Value = client
        .get("/openapi.json")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;

    let schema = &spec["paths"]["/webhook"]["post"]["requestBody"]["content"]["application/octet-stream"]
        ["schema"];
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["format"], "binary");

    site.shutdown_and_wait().await;
}

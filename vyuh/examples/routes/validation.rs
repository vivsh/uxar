//! Parse-only and validated request wrappers.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example routes_validation
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Validate, bundles,
    routes::{Json, Query, Valid},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,

    #[validate(min_length = 3)]
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Validate)]
struct UserSearch {
    #[validate(min_length = 2)]
    q: String,
}

#[bundles::route(path = "/parse-only", method = "POST")]
async fn parse_only(Json(input): Json<CreateUser>) -> Json<CreateUser> {
    Json(input)
}

#[bundles::route(path = "/users", method = "POST")]
async fn create_user(Valid(Json(input)): Valid<Json<CreateUser>>) -> Json<CreateUser> {
    Json(input)
}

#[bundles::route(path = "/users")]
async fn search_users(Valid(Query(input)): Valid<Query<UserSearch>>) -> Json<Vec<String>> {
    Json(vec![format!("search: {}", input.q)])
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle! {
        parse_only,
        create_user,
        search_users,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Validation example")
            .version("0.1.0")
            .spec("/openapi.json"),
    );

    assert!(bundle.reverse("create_user", &[]).is_some());
    println!("Use Json<T> to parse only; use Valid<Json<T>> to parse and validate.");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



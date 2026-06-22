//! OpenAPI response overrides and custom error schemas.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example openapi_responses
//! ```

use std::borrow::Cow;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    bundles,
    callables::PatchOp,
    routes::{Json, Methods, RouteConf},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Note {
    id: i64,
    title: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
struct CreateNote {
    title: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
struct ApiError {
    code: String,
    message: String,
}

// Macro sugar: `returns(...)` becomes the same operation metadata patch that
// direct registration applies with `PatchOp`.
#[bundles::route(
    path = "/notes",
    method = "POST",
    description = "Create a note.",
    returns(status = 201, description = "Created note"),
    returns(
        ty = "Json<ApiError>",
        status = 409,
        description = "A note with this title already exists"
    )
)]
async fn create_note(Json(input): Json<CreateNote>) -> Json<Note> {
    Json(Note {
        id: 1,
        title: input.title,
    })
}

async fn create_note_direct(Json(input): Json<CreateNote>) -> Json<Note> {
    Json(Note {
        id: 2,
        title: input.title,
    })
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let direct_route = bundles::route(
        create_note_direct,
        RouteConf {
            name: Cow::Borrowed("create_note_direct"),
            path: Cow::Borrowed("/direct/notes"),
            methods: Methods::POST,
            slash: None,
        },
    )
    .patch(
        PatchOp::new()
            .description("Create a note through direct registration.")
            .ret()
            .status(201)
            .doc("Created note")
            .done()
            .append()
            .status(409)
            .typed::<Json<ApiError>>()
            .doc("A note with this title already exists")
            .done(),
    );

    let bundle = bundles::bundle! {
        create_note,
    }
    .merge(bundles::bundle([direct_route]))
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Response Override API")
            .spec("/openapi.json"),
    );

    assert_eq!(
        bundle.reverse("create_note", &[]),
        Some("/notes".to_string())
    );
    assert_eq!(
        bundle.reverse("create_note_direct", &[]),
        Some("/direct/notes".to_string())
    );
    println!("OpenAPI response metadata includes macro and PatchOp overrides");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



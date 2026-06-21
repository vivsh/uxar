//! Dynamic authorization in handler logic.
//!
//! Run:
//!
//! ```sh
//! cargo run --example auth_dynamic_permission
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    Error, ErrorKind,
    auth::AuthUser,
    bundles,
    routes::{Json, Path},
};

#[derive(Debug, Serialize, JsonSchema)]
struct PostOut {
    id: u64,
    owner_id: &'static str,
}

#[bundles::route(path = "/posts/{id}", method = "PUT")]
async fn edit_post(user: AuthUser, Path(id): Path<u64>) -> Result<Json<PostOut>, Error> {
    let post = PostOut {
        id,
        owner_id: "user-123",
    };
    if post.owner_id != user.key.as_ref() {
        return Err(Error::new(ErrorKind::Forbidden).with_context("not allowed"));
    }
    Ok(Json(post))
}

fn main() {
    let bundle = bundles::bundle! {
        edit_post,
    };

    assert_eq!(
        bundle.reverse("edit_post", &[("id", "42")]),
        Some("/posts/42".to_string())
    );
    println!("dynamic permission route registered");
}

use axum::Json;
use uxar::{Path, views::route, bundles::IntoBundle};
use uxar_macros::bundle_impl;

struct UserApi;

#[bundle_impl(tags = ["api", "v1"])]
impl UserApi {
    /// Get user by ID
    #[route(method = "get", url = "/users/{id}")]
    async fn get_user(Path(id): Path<i32>) -> Json<String> {
        Json(format!("User {}", id))
    }

    /// Create a new user
    #[route(method = "post", url = "/users", tag = "users")]
    async fn create_user() -> Json<String> {
        Json("User created".to_string())
    }

    /// Update user
    #[route(method = "put", url = "/users/{id}", tag = "users")]
    async fn update_user(Path(id): Path<i32>) -> Json<String> {
        Json(format!("User {} updated", id))
    }
}

fn main() {
    // Use IntoBundle trait to get bundle
    let bundle = UserApi.into_bundle();
    let views: Vec<_> = bundle.iter_views().collect();

    println!("Bundle created with {} routes!", views.len());
    
    for view in views {
        println!("  - {} {} (tags: {:?})", 
            view.methods.iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join("|"),
            view.path,
            view.tags
        );
    }
}

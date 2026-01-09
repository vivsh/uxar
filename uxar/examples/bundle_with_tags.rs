use uxar::views::{route, Json};
use uxar::bundles::IntoBundle;
use uxar_macros::bundle_routes;

/// Get a greeting message
#[route(method = "GET", url = "/hello")]
async fn hello() -> Json<String> {
    Json("Hello, World!".to_string())
}

/// Create a new user
#[route(method = "POST", url = "/users", tag = "users")]
async fn create_user(Json(name): Json<String>) -> Json<String> {
    Json(format!("Created user: {}", name))
}

/// Update a user
#[route(method = "PUT", url = "/users/{id}")]
async fn update_user(id: uxar::Path<String>, Json(name): Json<String>) -> Json<String> {
    Json(format!("Updated user {} to {}", id.0, name))
}

fn main() {
    // Bundle without tags - routes keep their own tags
    let bundle1 = bundle_routes! {
        hello,
        create_user,
    };
    
    println!("Bundle 1 (no bundle-level tags):");
    for meta in bundle1.iter_views() {
        println!("  - {} (tags: {:?})", meta.name, meta.tags);
    }
    
    // Bundle with tags - extends individual route tags
    let bundle2 = bundle_routes! {
        tags = ["api", "v1"],
        hello,
        create_user,
        update_user,
    };
    
    println!("\nBundle 2 (with bundle-level tags):");
    for meta in bundle2.iter_views() {
        println!("  - {} (tags: {:?})", meta.name, meta.tags);
    }
    
    println!("\nBundle created successfully with {} routes!", bundle2.iter_views().count());
}

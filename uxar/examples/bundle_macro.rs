use uxar::views::{bundle, route, Json};
use uxar::bundles::{Bundle, IntoBundle};

/// Get a greeting message
#[route(method = "GET", url = "/hello")]
async fn hello() -> Json<String> {
    Json("Hello, World!".to_string())
}

/// Create a new user
#[route(method = "POST", url = "/users")]
async fn create_user(Json(name): Json<String>) -> Json<String> {
    Json(format!("Created user: {}", name))
}

/// Update a user
#[route(method = "PUT", url = "/users/{id}")]
async fn update_user(id: uxar::Path<String>, Json(name): Json<String>) -> Json<String> {
    Json(format!("Updated user {} to {}", id.0, name))
}

fn main() {
    // Use the bundle_routes! macro to collect routes
    let bundle = bundle_routes! {
        hello,
        create_user,
        update_user,
    };
    
    let metas: Vec<_> = bundle.iter_views().collect();
    println!("Created bundle with {} routes", metas.len());
    for meta in metas {
        println!("  - {} {} ({})", 
            meta.methods.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", "),
            meta.path,
            meta.name
        );
    }
    
    println!("\nBundle created successfully with {} routes!", bundle.iter_views().count());
}

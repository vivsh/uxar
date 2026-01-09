use uxar::views::{route, Json};

/// Get a greeting message
/// 
/// This endpoint returns a simple greeting
#[route(method = "GET", url = "/hello", tag = "greetings")]
async fn hello() -> Json<String> {
    Json("Hello, World!".to_string())
}

/// Create a new user
#[route(
    method = "POST",
    url = "/users",
    summary = "Create user",
    description = "Creates a new user account",
    tag = "users"
)]
async fn create_user(Json(name): Json<String>) -> Json<String> {
    Json(format!("Created user: {}", name))
}

struct UserController;

impl UserController {
    /// List all users
    #[route(method = "GET", url = "/api/users")]
    async fn list() -> Json<Vec<String>> {
        Json(vec!["Alice".to_string(), "Bob".to_string()])
    }

    /// Get a specific user
    #[route(method = "GET", url = "/api/users/{id}")]
    async fn get(id: uxar::Path<String>) -> Json<String> {
        Json(format!("User {}", id.0))
    }
}

fn main() {
    // Access the generated ViewMeta via functions
    let hello_meta = __route_meta_hello();
    println!("Route: {}", hello_meta.name);
    println!("Path: {}", hello_meta.path);
    println!("Methods: {:?}", hello_meta.methods);
    
    let create_user_meta = __route_meta_create_user();
    println!("\nRoute: {}", create_user_meta.name);
    println!("Path: {}", create_user_meta.path);
    
    // Access method ViewMeta through the impl
    let list_meta = UserController::__route_meta_list();
    println!("\nRoute: {}", list_meta.name);
    println!("Path: {}", list_meta.path);
    
    let get_meta = UserController::__route_meta_get();
    println!("\nRoute: {}", get_meta.name);
    println!("Path: {}", get_meta.path);
}

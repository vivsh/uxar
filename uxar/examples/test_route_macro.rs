use uxar::routes::*;
use uxar_macros::route;

#[route(method = "get", url = "/users/:id", arg(pos = 0, name = "id", description = "User ID"))]
async fn get_user(id: String) -> Json<String> {
    Json(format!("User {}", id))
}

fn main() {
    println!("Macro expansion test");
}

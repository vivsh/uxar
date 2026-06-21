//! Static role masks and permit! route guards.
//!
//! Run:
//!
//! ```sh
//! cargo run --example auth_roles_static
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    auth::{AuthUser, BitRole, permit},
    bundles,
    routes::Json,
};

#[derive(BitRole)]
enum AppRole {
    Manager,
    Editor,
    Viewer,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Secret {
    message: &'static str,
}

#[bundles::route(path = "/account")]
async fn account(user: AuthUser) -> Json<Secret> {
    let _ = user;
    Json(Secret {
        message: "authenticated",
    })
}

#[bundles::route(path = "/manage")]
async fn managers_only(_permit: permit!(AppRole, Manager)) -> Json<Secret> {
    Json(Secret {
        message: "managers only",
    })
}

#[bundles::route(path = "/edit")]
async fn editor_or_manager(_permit: permit!(AppRole, Manager | Editor)) -> Json<Secret> {
    Json(Secret {
        message: "editor or manager",
    })
}

fn main() {
    let manager = AuthUser::new(
        "manager-1",
        AppRole::Manager.to_role_type() | AppRole::Viewer.to_role_type(),
    );
    assert_ne!(manager.roles, 0);

    let bundle = bundles::bundle! {
        account,
        managers_only,
        editor_or_manager,
    };

    assert_eq!(
        bundle.reverse("managers_only", &[]),
        Some("/manage".to_string())
    );
    println!("static role auth routes registered");
}

use axum::{Json, extract::Query, http::StatusCode};
use serde::{Deserialize, Serialize};
use uxar::{
    Path, Site, SiteConf, Validate,
    bundles::IntoBundle,
    db::Model,
    permit,
    roles::{BitRole, Permit},
    schemables::{ApiDocGenerator, ApiMeta, DocViewer, TagInfo},
    views::{self, bundle_impl, bundle_routes, route},
};
use uxar_macros::{Filterable, Schemable};

#[derive(BitRole)]
#[repr(u8)]
pub enum UserRole {
    Admin = 1,
    Manager = 2,
    User = 3,
}

#[derive(Debug, Validate, Filterable, Schemable, Deserialize)]
pub struct UserFilter {
    pub is_active: Option<bool>,
    pub kind: Option<i16>,
}

#[derive(Debug)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Serialize, Deserialize, Model)]
struct User {
    id: i32,
    username: String,
    #[validate(email)]
    email: String,
    is_active: bool,
    kind: i16,
    /// User age with validation
    #[validate(min = 0, max = 100)]
    age: i32,
}

#[route(method = "get", url = "/greet")]
pub async fn greet() -> &'static str {
    "Hello, Uxar!"
}

#[route(method = "get", url = "/users")]
async fn list_staff() -> Json<Vec<User>> {
    let users = vec![
        User {
            id: 1,
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            is_active: true,
            kind: 1,
            age: 30,
        },
        User {
            id: 2,
            username: "bob".to_string(),
            email: "bob@example.com".to_string(),
            is_active: false,
            kind: 2,
            age: 45,
        },
    ];
    Json(users)
}

/// ViewSet for User-related endpoints
struct UserViewSet;

#[bundle_impl]
impl UserViewSet {
    /// Get user by ID
    #[route(method = "get", url = "/users/{user_id}")]
    async fn get_user(
        Path(user_id): Path<i32>,
        Permit(user, ..): permit!(UserRole, Admin | Manager),
    ) -> Json<User> {
        let user = User {
            id: user_id,
            username: "testuser".to_string(),
            email: "testuser@example.com".to_string(),
            is_active: true,
            kind: 1,
            age: 34,
        };
        Json(user)
    }

    /// List all usersss
    ///
    /// Returns a paginated list of all users in the system.
    /// This endpoint supports filtering, sorting, and pagination.
    ///
    /// ## Authentication
    /// Requires valid API key or session token.
    ///
    /// ## Rate Limiting
    /// Limited to 100 requests per minute per IP address.
    #[route(method = "get", url = "/users")]
    async fn list_users(
        q: Query<UserFilter>,
        permit: permit!(UserRole, Admin & Manager),
    ) -> Json<Vec<User>> {
        let users = vec![
            User {
                id: 1,
                username: "alice".to_string(),
                email: "alice@example.com".to_string(),
                is_active: true,
                kind: 1,
                age: 30,
            },
            User {
                id: 2,
                username: "bob".to_string(),
                email: "bob@example.com".to_string(),
                is_active: false,
                kind: 2,
                age: 45,
            },
        ];
        Json(users)
    }
}

#[tokio::main]
async fn main() {
    let mut viewset_bundle = UserViewSet.into_bundle();

    viewset_bundle = viewset_bundle.nest("/staff", "staff", bundle_routes!(greet, list_staff));

    let views: Vec<_> = viewset_bundle.iter_views().collect();

    let conf = SiteConf {
        ..SiteConf::from_env()
    };

    let api_meta = ApiMeta {
        title: "Uxar API".to_string(),
        version: "1.0.0".to_string(),
        description: Some("API documentation for Uxar application".to_string()),
        tags: vec![
            TagInfo {
                name: "Users".to_string(),
                description: Some("User management and authentication endpoints".to_string()),
            },
            TagInfo {
                name: "User Management".to_string(),
                description: Some(
                    "Operations for creating, updating, and managing user accounts".to_string(),
                ),
            },
        ],
    };

    viewset_bundle = viewset_bundle.with_api_spec_and_doc(
        "/openapi.json",
        "/docs/",
        api_meta,
        DocViewer::Rapidoc,
    );

    Site::builder(conf)
        .run(viewset_bundle)
        .await
        .expect("Failed to build site");
}

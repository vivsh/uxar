use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uxar::{
    Path, Site, SiteConf, db::{Bindable, Scannable, Schemable}, validation::Validate, views::{self, IntoResponse, Routable, routable, route}
};

#[derive(Debug, Schemable, Scannable, Bindable)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Serialize, Schemable, Deserialize, Scannable, Bindable)]
struct User {
    id: i32,
    username: String,
    email: String,
    is_active: bool,
    kind: i16,
}

async fn handle_sql(site: Site) -> views::Response {
    let db = site.db();
    let mut tx = db.begin().await.unwrap();

    let u = User {
        id: 1,
        username: "alice".to_string(),
        email: "asdad".to_string(),
        is_active: true,
        kind: 2,
    };

    let q = User::select_from("users_user").filter("kind = 1").count(&mut tx)
        .await
        .expect("asdasd asdada");

    println!("\n\nUser count with kind=1: {};\n\n", q);

    let users: Vec<User> = User::select_from("users_user")
        .filter("is_active AND kind = 1")
        .all(&mut tx)
        .await
        .map_err(|e| {
            println!("DB Error: {:?}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })
        .unwrap();

    views::Json(users).into_response()
}


pub struct Basket{
    pub items: Vec<String>,
    pub total: f64,
    pub price: f64,
    pub discount: f64,    
}

impl Validate for Basket {
    fn validate(&self) -> Result<(), uxar::validation::ValidationReport> {
        let mut errors = uxar::validation::ValidationReport::empty();

        if self.items.is_empty() {
            errors.push_root(uxar::validation::ValidationError::new("items", "Basket must contain at least one item"));
        }

        if self.total < 110.0 {
            errors.push_root(uxar::validation::ValidationError::new("total", "Total price cannot be negative"));
        }

        if self.discount < 0.0 || self.discount > self.price {
            errors.push_root(uxar::validation::ValidationError::new("discount", "Discount must be between 0 and the price"));
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

struct UserView;

#[routable]
impl UserView{

    #[route(url="/users/{path}")]
    async fn list_users(Path(path): Path<i32>) -> views::Response {
        views::Html("<h1>User List</h1>".to_string()).into_response()
    }


}

#[tokio::main]
async fn main() {

    let basket = Basket {
        items: vec!["apple".to_string(), "banana".to_string()],
        total: 15.0,
        price: 20.0,
        discount: 5.0,
    };
    println!("Starting Uxar site...{:?}", basket.validate().unwrap_err().to_nested_map());

    println!("Starting Uxar site... {:?}", UserView::as_routable().1);

    let conf = SiteConf {
        ..SiteConf::from_env()
    };

    let router = views::AxumRouter::new().fallback(|| async { "<h1>Hello, Uxar!</h1>" });

    let router2 = views::AxumRouter::new().route("/earth/", views::get(handle_sql));

    Site::builder(conf)
        .merge(router)
        .merge(UserView::as_routable())
        .mount("/world", "name", router2)
        .run()
        .await
        .expect("Failed to build site");
}

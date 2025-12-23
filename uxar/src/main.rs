use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uxar::{
    Path, Site, SiteConf, db::{Model, Schemable}, validation::Validate, views::{self, IntoResponse, Routable, routable, route}

};

#[derive(Debug, Schemable)]
struct Address {
    street: String,
    city: String,
    zip: String,
}

#[derive(Debug, Serialize,Deserialize, Schemable)]
#[schemable(name = "users_user")]
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

    let q = User::to_select().filter("kind = 1").count(&mut tx)
        .await
        .expect("asdasd asdada");

    println!("\n\nUser count with kind=1: {};\n\n", q);

    let users: Vec<User> = User::to_select()
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

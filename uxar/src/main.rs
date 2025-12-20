use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uxar::{
    Path, Site, SiteConf, db::{Bindable, Scannable, Schemable}, views::{self, IntoResponse, Viewable, action, viewable}
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


struct UserView;

#[viewable]
impl UserView{

    #[action]
    async fn list_users(path: Path<i32>) -> views::Response {
        views::Html("<h1>User List</h1>".to_string()).into_response()
    }


}

#[tokio::main]
async fn main() {
    // let dir: Dir = embed!("tests");
    // let dirset = DirSet::new(vec![dir]);
    // for d in dirset.walk() {
    //     println!("Entry: {:?}", d.path());
    //     {
    //         println!("File: {:?}", d.base_name());
    //         println!("Path: {}", d.path().display());
    //         println!("Contents: {:?}", d.read_bytes_async().await.unwrap().len());
    //     }
    // }

    println!("Starting Uxar site... {:?}", UserView::describe_routes());

    let conf = SiteConf {
        ..SiteConf::from_env()
    };

    let router = views::Router::new().fallback(|| async { "<h1>Hello, Uxar!</h1>" });

    let router2 = views::Router::new().route("/earth/", views::get(handle_sql));

    Site::builder(conf)
        .mount("", router)
        .mount("/world", router2)
        .run()
        .await
        .expect("Failed to build site");
}

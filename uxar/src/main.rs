use axum::{response::Response, Router};
use uxar::{
    db, embed::{self, embed, Dir, DirSet, Entry}, Application, IntoApplication, Site, SiteConf
};


async fn handle_sql(site: Site) -> Response{
    let query = "SELECT * FROM users WHERE id = $1";
    return uxar::db::jsql_all(site.db().pool(), sqlx::query(query)).await;
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

    let conf = SiteConf {
        ..SiteConf::from_env()
    };

    let router = Router::new().fallback(|| async { "<h1>Hello, Uxar!</h1>" });

    let router2 = Router::new().route(
        "/earth/",
        axum::routing::get(|| async { "<h1>Fallback Route</h1>" }),
    );

    Site::builder(conf)
        .mount("", router)
        .mount("/world", router2)
        .run()
        .await
        .expect("Failed to build site");
}

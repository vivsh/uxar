
use axum::Router;
use uxar::{Application, IntoApplication, Site, SiteConf};


#[tokio::main]
async fn main() {
    
    let conf = SiteConf{
        host: "localhost".into(),
        port: 8080,
        database: "postgres:///uxar".into(),
        ..Default::default()
    };

    let router = Router::<Site>::new().fallback(|| async {
        "<h1>Hello, Uxar!</h1>"
    });


    let router2 = Router::<Site>::new().route("/earth/", axum::routing::get(|| async { "<h1>Fallback Route</h1>" }));

    Site::builder(conf)
    .mount("", router)
    .mount("/world", router2)
    .run().await
        .expect("Failed to build site");

}

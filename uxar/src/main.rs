use axum::Router;
use uxar::{embed::{Entry, Dir, embed}, Application, IntoApplication, Site, SiteConf, embed};



#[tokio::main]
async fn main() {
    let dir: Dir = embed!("tests", true);
    for d in dir.entries() {
        println!("Entry: {:?}", d.path());
        if let Entry::File(file) = d {
            println!("File: {:?}", file.base_name());
            println!("Path: {}", file.path().display());
            println!("Contents: {:?}", file.read_bytes_async().await.unwrap().len());
        }
    }


    
    // let conf = SiteConf{
    //     host: "localhost".into(),
    //     port: 8080,
    //     database: "postgres:///uxar".into(),
    //     ..Default::default()
    // };

    // let router = Router::<Site>::new().fallback(|| async {
    //     "<h1>Hello, Uxar!</h1>"
    // });


    // let router2 = Router::<Site>::new().route("/earth/", axum::routing::get(|| async { "<h1>Fallback Route</h1>" }));

    // Site::builder(conf)
    // .mount("", router)
    // .mount("/world", router2)
    // .run().await
    //     .expect("Failed to build site");

}

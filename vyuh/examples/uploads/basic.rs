//! Basic typed multipart upload.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example uploads_basic
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    SiteConf, bundles,
    routes::{Json, MultipartForm, UploadedFile},
};

#[derive(Debug, JsonSchema, vyuh::MultipartData)]
struct UploadAvatar {
    avatar: UploadedFile,
}

#[derive(Debug, Serialize, JsonSchema)]
struct UploadOut {
    size: u64,
}

#[bundles::route(path = "/avatar", method = "POST")]
async fn upload_avatar(MultipartForm(input): MultipartForm<UploadAvatar>) -> Json<UploadOut> {
    Json(UploadOut {
        size: input.avatar.size(),
    })
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let _site_conf = SiteConf::default();
    let bundle = bundles::bundle! {
        upload_avatar,
    };
    assert_eq!(bundle.reverse("upload_avatar", &[]), Some("/avatar".into()));
    println!("registered basic upload route");
    example_common::run_example(bundle).await
}
#[path = "../common.rs"] mod example_common;



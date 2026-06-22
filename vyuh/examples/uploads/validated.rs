//! Multipart upload with MIME, extension, sniffing, and size checks.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example uploads_validated
//! ```

use schemars::JsonSchema;
use serde::Serialize;
use vyuh::{
    SiteConf, bundles,
    routes::{Json, MultipartForm, UploadedFile},
};

#[derive(Debug, JsonSchema, vyuh::MultipartData)]
struct AvatarUpload {
    display_name: String,
    #[upload(
        content_types = ["image/png", "image/jpeg", "image/webp"],
        extensions = ["png", "jpg", "jpeg", "webp"],
        sniff = "image",
        max_size = 2_000_000
    )]
    avatar: UploadedFile,
}

#[derive(Debug, Serialize, JsonSchema)]
struct UploadOut {
    display_name: String,
    size: u64,
    detected: Option<String>,
}

#[bundles::route(path = "/avatar", method = "POST")]
async fn upload_avatar(MultipartForm(input): MultipartForm<AvatarUpload>) -> Json<UploadOut> {
    Json(UploadOut {
        display_name: input.display_name,
        size: input.avatar.size(),
        detected: input.avatar.sniffed_content_type().map(ToOwned::to_owned),
    })
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default();
    let bundle = bundles::bundle! {
        upload_avatar,
    };
    assert_eq!(bundle.reverse("upload_avatar", &[]), Some("/avatar".into()));
    println!("registered validated upload route");
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



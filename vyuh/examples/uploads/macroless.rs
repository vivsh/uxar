//! Macro-less multipart upload handling with `MultipartMap` and `MultipartSpec`.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example uploads_macroless
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    Data, Error, Site, SiteConf, bundles,
    routes::multipart::{FieldRule, FileRule, MultipartMap, MultipartSpec},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UploadOut {
    url: Option<String>,
    size: u64,
}

#[bundles::route(path = "/avatar", method = "POST")]
async fn upload_avatar(site: Site, form: MultipartMap) -> Result<Data<UploadOut>, Error> {
    let form = form.validate(
        MultipartSpec::new()
            .text("display_name", FieldRule::new().required().max_length(80))
            .file(
                "avatar",
                FileRule::new()
                    .required()
                    .content_types(["image/png"])
                    .extensions(["png"])
                    .sniff_image()
                    .max_size(2_000_000),
            ),
    )?;
    let avatar = form.file("avatar")?;
    let saved = site.file_storage().save(avatar).await?;
    Ok(Data::new(UploadOut {
        url: saved.url,
        size: avatar.size(),
    }))
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let conf = SiteConf::default();
    let bundle = bundles::bundle! {
        upload_avatar,
    };
    assert_eq!(bundle.reverse("upload_avatar", &[]), Some("/avatar".into()));
    println!("registered macro-less upload route");
    example_common::run_example_with_conf(conf, bundle).await
}
#[path = "../common.rs"] mod example_common;



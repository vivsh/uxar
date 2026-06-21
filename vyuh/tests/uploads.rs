use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vyuh::file_storage::{StorageName, UploadConf};
use vyuh::routes::multipart::{FieldRule, FileRule, MultipartMap, MultipartSpec, UploadedFile};
use vyuh::routes::{Json, MultipartForm, StatusCode};
use vyuh::{Data, Error, Site, SiteConf, Validate, bundles};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UploadOut {
    name: String,
    size: u64,
    sniffed: Option<String>,
}

#[derive(Debug, Clone, JsonSchema, vyuh::MultipartData)]
struct AvatarUpload {
    display_name: String,
    #[upload(
        content_types = ["image/png"],
        extensions = ["png"],
        sniff = "image",
        max_size = 64
    )]
    avatar: UploadedFile,
}

impl Validate for AvatarUpload {
    fn validate(&self) -> Result<(), vyuh::ValidationReport> {
        Ok(())
    }
}

impl vyuh::ValidationSchema for AvatarUpload {
    fn apply_validation_schema(
        _schema: &mut serde_json::Value,
        _definitions: &mut serde_json::Map<String, serde_json::Value>,
    ) {
    }
}

#[bundles::route(path = "/typed", method = "POST")]
async fn typed_upload(MultipartForm(input): MultipartForm<AvatarUpload>) -> Json<UploadOut> {
    Json(UploadOut {
        name: input.display_name,
        size: input.avatar.size(),
        sniffed: input.avatar.sniffed_content_type().map(ToOwned::to_owned),
    })
}

#[bundles::route(path = "/macro-less", method = "POST")]
async fn macro_less_upload(site: Site, form: MultipartMap) -> Result<Data<UploadOut>, Error> {
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
                    .max_size(64),
            ),
    )?;
    let avatar = form.file("avatar")?;
    let saved = site.file_storage().save(avatar).await?;
    Ok(Data::new(UploadOut {
        name: saved.name.to_string(),
        size: avatar.size(),
        sniffed: avatar.sniffed_content_type().map(ToOwned::to_owned),
    }))
}

fn multipart_body(boundary: &str, file_name: &str, content_type: &str, file: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"display_name\"\r\n\r\nViv\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"avatar\"; filename=\"{file_name}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

fn png_bytes() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    bytes.extend_from_slice(&[0; 16]);
    bytes
}

async fn upload_site() -> (vyuh::Site, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let conf = SiteConf {
        log_init: false,
        project_dir: dir.path().to_string_lossy().to_string(),
        uploads: UploadConf {
            dir: "uploads".into(),
            temp_dir: Some("tmp".into()),
            memory_threshold_bytes: 8,
            max_request_bytes: 512,
            max_file_bytes: 128,
            ..UploadConf::default()
        },
        logging: vyuh::logging::LoggingConf {
            env_prefix: None,
            rules: vec![],
        },
        ..SiteConf::default()
    };
    let site = vyuh::Site::build(
        conf,
        bundles::bundle! {
            typed_upload,
            macro_less_upload,
        },
    )
    .await
    .unwrap();
    (site, dir)
}

async fn upload_openapi_site() -> (vyuh::Site, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let conf = SiteConf {
        log_init: false,
        project_dir: dir.path().to_string_lossy().to_string(),
        logging: vyuh::logging::LoggingConf {
            env_prefix: None,
            rules: vec![],
        },
        ..SiteConf::default()
    };
    let bundle = bundles::bundle! {
        typed_upload,
    }
    .with_openapi(
        bundles::OpenApiConf::default()
            .title("Uploads")
            .version("0.1.0")
            .spec("/openapi.json"),
    );
    let site = vyuh::Site::build(conf, bundle).await.unwrap();
    (site, dir)
}

#[tokio::test]
async fn typed_multipart_accepts_sniffed_png() {
    let (site, _dir) = upload_site().await;
    let client = vyuh::testing::TestClient::new(site.clone());
    let boundary = "vyuh-boundary";
    let body = multipart_body(boundary, "avatar.png", "image/png", &png_bytes());

    let out: UploadOut = client
        .post("/typed")
        .header(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(body))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;

    assert_eq!(out.name, "Viv");
    assert_eq!(out.sniffed.as_deref(), Some("image/png"));
    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn macro_less_upload_saves_file_with_local_storage() {
    let (site, dir) = upload_site().await;
    let client = vyuh::testing::TestClient::new(site.clone());
    let boundary = "vyuh-boundary";
    let body = multipart_body(boundary, "avatar.png", "image/png", &png_bytes());

    let out: UploadOut = client
        .post("/macro-less")
        .header(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(body))
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;

    assert!(dir.path().join("uploads").join(&out.name).exists());
    assert_eq!(out.size, png_bytes().len() as u64);
    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn invalid_sniffed_file_is_rejected() {
    let (site, _dir) = upload_site().await;
    let client = vyuh::testing::TestClient::new(site.clone());
    let boundary = "vyuh-boundary";
    let body = multipart_body(boundary, "avatar.png", "image/png", b"not an image");

    let body: Value = client
        .post("/typed")
        .header(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(body))
        .send()
        .await
        .assert_status(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        .json()
        .await;

    assert_eq!(body["code"], "unsupported_upload");
    site.shutdown_and_wait().await;
}

#[tokio::test]
async fn oversized_file_is_rejected() {
    let (site, _dir) = upload_site().await;
    let client = vyuh::testing::TestClient::new(site.clone());
    let boundary = "vyuh-boundary";
    let body = multipart_body(boundary, "avatar.png", "image/png", &[0x89; 80]);

    client
        .post("/typed")
        .header(
            "content-type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .body(axum::body::Body::from(body))
        .send()
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);

    site.shutdown_and_wait().await;
}

#[test]
fn unsafe_storage_names_are_rejected() {
    assert!(StorageName::new("../avatar.png").is_err());
    assert!(StorageName::new("/avatar.png").is_err());
    assert!(StorageName::new("nested\\avatar.png").is_err());
    assert!(StorageName::new("avatar.png").is_ok());
}

#[tokio::test]
async fn multipart_openapi_documents_binary_file_field() {
    let (site, _dir) = upload_openapi_site().await;
    let client = vyuh::testing::TestClient::new(site.clone());

    let spec: Value = client
        .get("/openapi.json")
        .send()
        .await
        .assert_status(StatusCode::OK)
        .json()
        .await;

    let schema =
        &spec["paths"]["/typed"]["post"]["requestBody"]["content"]["multipart/form-data"]["schema"];
    assert_eq!(schema["$ref"], "#/components/schemas/AvatarUpload");
    let avatar = &spec["components"]["schemas"]["AvatarUpload"]["properties"]["avatar"];
    assert_eq!(avatar["type"], "string");
    assert_eq!(avatar["format"], "binary");

    site.shutdown_and_wait().await;
}

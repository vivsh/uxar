# Uploads

Vyuh handles file uploads through route multipart wrappers and site file
storage. Multipart parsing belongs to `vyuh::routes`; uploaded/runtime file
persistence belongs to `site.file_storage()`.

Uploads are separate from assets. Assets are code-shipped files served from
bundles and copied by `collect_static`. Uploads are runtime/user files stored by
the configured file storage backend.

## Overview

The main public pieces are:

- `MultipartForm<T>` for typed `multipart/form-data` requests.
- `MultipartData` for mapping a multipart request into a typed struct.
- `MultipartMap`, `MultipartSpec`, `FieldRule`, and `FileRule` for macro-less
  upload handling.
- `UploadedFile`, `UploadedText`, and `JsonPart<T>` for parsed multipart parts.
- `SiteConf::uploads(UploadConf)` for upload limits and local paths.
- `site.file_storage()` for saving accepted files.
- `LocalStorage` as the default file storage backend.

## Configuration

Configure upload limits and local storage paths on `SiteConf`:

```rust
use vyuh::{SiteConf, file_storage::UploadConf};

let conf = SiteConf::default().uploads(UploadConf {
    dir: "media/uploads".into(),
    base_url: Some("/media/uploads".into()),
    temp_dir: Some("tmp/uploads".into()),
    max_request_bytes: 25 * 1024 * 1024,
    max_file_bytes: 10 * 1024 * 1024,
    max_files: 20,
    max_fields: 100,
    memory_threshold_bytes: 256 * 1024,
});
```

Small uploads stay in memory until `memory_threshold_bytes`. Larger uploads are
spooled to temporary files while the request is parsed. Oversized uploads stop
streaming and return `413`.

## Typed Uploads

Typed multipart parsing uses `MultipartData`:

```rust
use vyuh::routes::{MultipartForm, UploadedFile};

#[derive(schemars::JsonSchema, vyuh::MultipartData)]
struct AvatarUpload {
    display_name: String,
    #[upload(
        content_types = ["image/png", "image/jpeg"],
        extensions = ["png", "jpg", "jpeg"],
        sniff = "image",
        max_size = 2_000_000
    )]
    avatar: UploadedFile,
}

async fn upload(MultipartForm(input): MultipartForm<AvatarUpload>) {
    // input.avatar has passed the multipart rules above.
}
```

The derive generates the same `MultipartData` implementation that can be
written manually. The runtime API does not require macros.

## Macro-less Uploads

Use `MultipartMap` directly when the accepted fields are dynamic or when direct
registration is clearer:

```rust
use vyuh::{
    Data, Error, Site,
    routes::multipart::{FileRule, FieldRule, MultipartMap, MultipartSpec},
};

async fn upload_avatar(site: Site, form: MultipartMap) -> Result<Data<UploadOut>, Error> {
    let form = form.validate(
        MultipartSpec::new()
            .text("display_name", FieldRule::new().required().max_length(80))
            .file(
                "avatar",
                FileRule::new()
                    .required()
                    .content_types(["image/png", "image/jpeg"])
                    .extensions(["png", "jpg", "jpeg"])
                    .sniff_image()
                    .max_size(2_000_000),
            ),
    )?;

    let saved = site.file_storage().save(form.file("avatar")?).await?;
    Ok(Data::new(UploadOut { url: saved.url }))
}
```

`MultipartMap` parses the request first because it does not know the handler's
rules until `validate(...)` is called. `MultipartForm<T>` can reject disallowed
field names, declared content types, and extensions while streaming because the
spec is known before parsing.

## MIME Screening

Vyuh treats these as separate checks:

- declared multipart `Content-Type`;
- filename extension;
- sniffed MIME type from uploaded bytes.

`sniff_image()` uses the `infer` crate against a bounded byte prefix. It can
reject invalid image content before the full file is accepted or saved, but it
must read a small prefix first.

```rust
FileRule::new()
    .content_types(["image/png"])
    .extensions(["png"])
    .sniff_image()
```

Use `SniffRule::mime([...])` when the accepted sniffed MIME types are not just
images.

## File Storage

Save accepted files through `site.file_storage()`:

```rust
let saved = site.file_storage().save(&input.avatar).await?;
```

`LocalStorage` writes under `UploadConf::dir`. Generated names are
collision-resistant. `save_as(...)` accepts only validated `StorageName` values:
absolute paths, `..`, backslashes, NUL bytes, and empty names are rejected.

`UploadedFile::file_name()` is client metadata only. Do not use it directly as a
storage name.

## Errors

Multipart failures use the normal Vyuh error pipeline:

- malformed multipart returns `400`;
- unsupported declared or sniffed file type returns `415`;
- request or file size limit failures return `413`;
- validation failures return `422`;
- storage failures become `vyuh::Error`.

HTTP responses are rendered through the configured `ErrorView`/`ErrorReport`
handlers. See [Errors](errors.md).

## OpenAPI

`MultipartForm<T>` documents a `multipart/form-data` request body.
`UploadedFile` fields are rendered as binary string fields:

```yaml
type: string
format: binary
```

Validation metadata is published only when a route uses
`Valid<MultipartForm<T>>`.

## Examples

- [`uploads_basic.rs`](../vyuh/examples/uploads_basic.rs): basic typed upload.
- [`uploads_validated.rs`](../vyuh/examples/uploads_validated.rs): MIME,
  extension, sniffing, and size checks.
- [`uploads_macroless.rs`](../vyuh/examples/uploads_macroless.rs):
  `MultipartMap` and `MultipartSpec`.
- [`uploads_large.rs`](../vyuh/examples/uploads_large.rs): large upload
  configuration.

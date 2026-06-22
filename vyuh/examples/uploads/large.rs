//! Large upload configuration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example uploads_large
//! ```

use vyuh::{SiteConf, file_storage::UploadConf};

fn main() {
    let conf = SiteConf::default().uploads(UploadConf {
        dir: "media/uploads".into(),
        base_url: Some("/media/uploads".into()),
        temp_dir: Some("tmp/uploads".into()),
        max_request_bytes: 250 * 1024 * 1024,
        max_file_bytes: 100 * 1024 * 1024,
        max_files: 8,
        max_fields: 32,
        memory_threshold_bytes: 512 * 1024,
    });

    assert_eq!(conf.uploads.max_file_bytes, 100 * 1024 * 1024);
    assert_eq!(conf.uploads.temp_dir.as_deref(), Some("tmp/uploads"));
    println!("configured large upload limits");
}

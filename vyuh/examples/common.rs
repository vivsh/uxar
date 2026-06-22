use std::path::PathBuf;

use tempfile::{TempDir, tempdir};

pub async fn run_example(bundle: impl vyuh::bundles::IntoBundle) -> Result<(), vyuh::SiteError> {
    let conf = vyuh::SiteConf::from_env_with_files().map_err(vyuh::SiteError::from)?;
    run_example_with_conf(conf, bundle).await
}

pub async fn run_example_with_conf(
    mut conf: vyuh::SiteConf,
    bundle: impl vyuh::bundles::IntoBundle,
) -> Result<(), vyuh::SiteError> {
    let temp_dir: TempDir = tempdir().map_err(vyuh::SiteError::from)?;
    let db_path: PathBuf = temp_dir.path().join("vyuh-example.sqlite");
    let db_url = format!("sqlite://{}", db_path.to_string_lossy());
    conf = conf
        .project_dir(temp_dir.path().to_string_lossy().to_string())
        .database(vyuh::db::DbConf::from_url(&db_url).map_err(vyuh::SiteError::from)?)
        .port(0);

    // Keep temporary directory alive while the example is running.
    let _temp_dir = temp_dir;
    vyuh::Site::run(conf, bundle).await
}

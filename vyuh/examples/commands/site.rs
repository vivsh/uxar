//! A command that extracts Site and inspects runtime state.
//!
//! Run:
//!
//! ```sh
//! cargo run --example commands_site inspect --project
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, Site, SiteConf, bundles, commands::CommandConf};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct InspectArgs {
    /// Print the configured project directory.
    #[serde(default)]
    project: bool,
}

/// Inspect the built site.
async fn inspect(site: Site, Data(args): Data<InspectArgs>) -> Result<(), Error> {
    println!("timezone: {}", site.timezone());
    if args.project {
        println!("project_dir: {}", site.project_dir().display());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle([bundles::command(
        inspect,
        CommandConf::new("inspect").description("Inspect site runtime state."),
    )]);
    vyuh::Site::run(SiteConf::from_env_with_files()?, bundle).await
}

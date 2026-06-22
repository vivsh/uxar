//! Typed command arguments with direct command registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example commands_macro greet --name Vyuh --verbose
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{Data, Error, SiteConf, bundles, commands::CommandConf};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct GreetArgs {
    /// Name to greet.
    name: String,
    /// Print extra output.
    #[serde(default)]
    verbose: bool,
}

/// Print a greeting.
async fn greet(Data(args): Data<GreetArgs>) -> Result<(), Error> {
    if args.verbose {
        println!("running greet command");
    }
    println!("hello {}", args.name);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), vyuh::SiteError> {
    let bundle = bundles::bundle([bundles::command(
        greet,
        CommandConf::new("greet").description("Print a greeting."),
    )]);
    vyuh::Site::run(SiteConf::from_env_with_files()?, bundle).await
}

#[path = "../common.rs"] mod example_common;
//! Typed command arguments with direct command registration.
//!
//! Run:
//!
//! ```sh
//! cargo run -p vyuh --no-default-features --features sqlite --example commands_macro greet --name Vyuh --verbose
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
    let conf = SiteConf::from_env_with_files()?;
    example_common::run_example_with_conf(conf, bundle).await
}

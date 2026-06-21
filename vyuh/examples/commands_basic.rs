//! Typed command arguments with direct command registration.
//!
//! Run:
//!
//! ```sh
//! cargo run --example commands_basic greet --name Vyuh --verbose
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vyuh::{
    SiteConf, bundles,
    commands::{CommandArgs, CommandConf, CommandError},
};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
struct GreetArgs {
    /// Name to greet.
    name: String,
    /// Print extra output.
    #[serde(default)]
    verbose: bool,
}

/// Print a greeting.
async fn greet(args: CommandArgs<GreetArgs>) -> Result<(), CommandError> {
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
    vyuh::run_command(SiteConf::from_env_with_files()?, bundle).await
}

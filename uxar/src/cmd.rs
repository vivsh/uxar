
use argh::{FromArgs};

#[derive(FromArgs, PartialEq, Eq, Debug, Clone)]
/// Site commands
pub struct SiteCommand {
    #[argh(subcommand)]
    pub nested: NestedCommand,

    #[argh(switch, short = 'v', long = "verbose")]
    /// enable verbose output
    pub verbose: bool,
}


#[derive(FromArgs, PartialEq, Eq, Debug, Clone)]
#[argh(subcommand)]
pub enum NestedCommand{
    Serve(ServeCommand),
    Init(InitCommand),
}

#[derive(FromArgs, PartialEq, Eq, Debug, Clone)]
#[argh(subcommand, name = "serve")]
/// Serve the site
pub struct ServeCommand {
    #[argh(option, default = "\"localhost\".into()")]
    /// host to bind the server to
    host: String,

    #[argh(option, default="8080")]
    /// port to bind the server to
    port: u16,
}

pub struct Migrate{

}


#[derive(FromArgs, PartialEq, Eq, Debug, Clone)]
#[argh(subcommand, name = "init")]
/// Initialize the site
pub struct InitCommand{

}


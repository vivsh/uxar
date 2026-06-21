mod conf;
mod host;
mod site;

extern crate self as vyuh;

pub mod apidocs;
pub mod assets;
pub mod auth;
pub mod beacon;
pub mod bundles;
pub mod callables;
pub mod channels;
pub mod commands;
pub mod db;
pub mod embed;
pub mod emitters;
pub mod errors;
pub mod logging;
pub mod middlewares;
pub(crate) mod notifiers;
pub(crate) mod roles;
pub(crate) mod schedulers;
pub mod services;
pub mod signals;
pub mod tasks;
pub mod templates;
pub mod testing;
pub mod validation;
pub mod validators;
mod watch;

#[cfg(test)]
mod testing_tests;

pub mod routes;
pub use callables::{Operation, OperationKind};
pub use commands::CommandError;
pub use conf::{SiteConf, StaticDir};
pub use host::HostService;
pub use site::{Site, SiteError, build_site, run_command, serve_site, test_site};
pub use validation::{
    Valid, ValidRejection, Validate, ValidationError, ValidationReport, ValidationSchema,
};

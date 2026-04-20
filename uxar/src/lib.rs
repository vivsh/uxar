mod site;
mod conf;
mod cmd;
mod host;

extern crate self as uxar;

pub(crate) mod templates;
pub mod services;
pub mod assets;
pub mod errors;
pub(crate) mod debounce;
pub(crate) mod schedulers;
pub mod channels;
pub(crate) mod notifiers;
pub mod auth;
mod commands;
pub(crate) mod roles;
pub mod validation;
pub mod validators;
pub mod logging;
mod layers;
pub mod tasks;
pub mod admin;
mod watch;
pub mod callables;
pub mod emitters;
pub mod apidocs;
pub mod beacon;
pub mod signals;
pub mod zones;
pub mod bundles; 
pub mod db;
pub mod embed;
pub mod testing;

#[cfg(test)]
mod testing_tests;

pub mod routes;
pub use site::{Site, SiteError, build_site, serve_site, test_site};
pub use conf::{SiteConf, StaticDir};
pub use host::{HostService};
pub use validation::{Valid, ValidRejection, Validate, ValidationReport, ValidationError};
pub use callables::{Operation, OperationKind};

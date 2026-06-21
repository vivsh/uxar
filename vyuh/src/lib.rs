#![allow(
    clippy::await_holding_lock,
    clippy::collapsible_else_if,
    clippy::collapsible_if,
    clippy::derivable_impls,
    clippy::doc_lazy_continuation,
    clippy::empty_line_after_doc_comments,
    clippy::explicit_auto_deref,
    clippy::inherent_to_string,
    clippy::io_other_error,
    clippy::large_enum_variant,
    clippy::let_and_return,
    clippy::manual_div_ceil,
    clippy::manual_is_ascii_check,
    clippy::match_result_ok,
    clippy::module_inception,
    clippy::multiple_bound_locations,
    clippy::needless_borrow,
    clippy::needless_return,
    clippy::new_without_default,
    clippy::ptr_arg,
    clippy::redundant_closure,
    clippy::redundant_field_names,
    clippy::redundant_pattern_matching,
    clippy::result_large_err,
    clippy::should_implement_trait,
    clippy::single_component_path_imports,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::useless_conversion,
    clippy::useless_vec,
    clippy::clone_on_copy,
    clippy::extra_unused_lifetimes,
    clippy::for_kv_map,
    clippy::get_first,
    clippy::unnecessary_lazy_evaluations,
    clippy::unnecessary_map_or,
    clippy::unwrap_or_default
)]

mod conf;
mod site;

extern crate self as vyuh;

pub mod apidocs;
pub mod assets;
pub mod auth;
pub mod bundles;
pub mod callables;
pub mod channels;
pub mod commands;
pub mod db;
pub mod embed;
pub mod emitters;
pub mod errors;
pub mod file_storage;
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
pub use callables::{Data, DataValue, Operation, OperationKind};
pub use commands::CommandError;
pub use conf::{SiteConf, StaticDir};
pub use errors::{
    Error, ErrorCommandContext, ErrorContext, ErrorKind, ErrorRenderContext, ErrorRenderTarget,
    ErrorReport, ErrorRequestContext, ErrorSourceKind, ErrorView, HttpErrorRenderMode,
};
pub use file_storage::{
    FileStorageError, LocalStorage, SavedFile, StorageBackend, StorageName, UploadConf,
};
pub use site::{Site, SiteConfig, SiteError};
pub use validation::{
    Valid, ValidRejection, Validate, ValidationError, ValidationReport, ValidationSchema,
};
pub use vyuh_macros::MultipartData;

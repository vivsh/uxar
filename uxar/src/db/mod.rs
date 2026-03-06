
mod executor;
mod query;
mod interfaces;
mod migrations;
mod models;
mod commons;
mod argvalue;
mod scopes;
mod placeholders;

pub mod mock;

pub use commons::{Database, Arguments, Row, QueryResult, Pool};
pub use argvalue::ArgValue;
pub use scopes::Scope;
pub use query::{QuerySet, Statement};
pub use uxar_macros::{Filterable, Scannable, Bindable};
pub use executor::*;
pub use interfaces::{
    Filterable, Recordable, Scannable, Bindable, Model,
    rust_to_pg_type,
};
pub use models::{TableModel, ColumnModel};

pub use sqlx::test as test_db;

// Re-export sqlx types so macros don't need direct sqlx dependency
pub use sqlx;

// Re-export commonly used sqlx types for convenience
pub use sqlx::{
    Error,
    FromRow,
    Encode,
    Decode,
    Type,
    Row as RowTrait,
};

// Re-export serde_json for macro use
pub use serde_json;

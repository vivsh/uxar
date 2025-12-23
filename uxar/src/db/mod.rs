
mod executor;
mod query;
mod interfaces;
mod migrations;
mod models;

pub use query::{Statement,};
pub use uxar_macros::{Model, Filterable};
pub use executor::*;
pub use interfaces::{
    ColumnSpec, ColumnValidation, SchemaInfo, ColumnKind, 
    Scannable, Bindable, Filterable, Model,
    rust_to_pg_type,
};
pub use models::{TableModel, ColumnModel};

// Re-export sqlx types so macros don't need direct sqlx dependency
pub use sqlx;

// Re-export commonly used sqlx types for convenience
pub use sqlx::{
    Error as SqlxError,
    Row,
    FromRow,
    Encode,
    Decode,
    Type,
    Arguments,
    postgres::{PgRow, PgArguments, Postgres},
};

// Re-export serde types used by macros
pub use serde;
pub use serde_json;
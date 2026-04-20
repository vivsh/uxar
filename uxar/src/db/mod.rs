
mod executor;
mod interfaces;
mod commons;
mod argvalue;
mod scopes;
mod placeholders;
pub(crate) mod queries;

pub mod mock;

pub use commons::{Database, Arguments, Row, QueryResult, Pool};
pub use argvalue::ArgValue;
pub use scopes::Scope;
pub use queries::{Statement, Page, LockMode, FilteredBuilder, QueryError};
pub use queries::{SelectQuery, InsertQuery, UpdateQuery, DeleteQuery};
pub use uxar_macros::{Filterable, Scannable, Bindable};
pub use executor::*;
pub use interfaces::{
    Filterable, Recordable, Scannable, Bindable, Model,
    rust_to_pg_type,
};
pub use sqlx::test as test_db;

/// Start a SELECT query against `table`.
pub fn select(table: &str) -> Result<SelectQuery, DbError> {
    queries::validate_ident(table)?;
    Ok(SelectQuery::new(table))
}

/// Start an INSERT INTO `table` query.
pub fn insert(table: &str) -> Result<InsertQuery, DbError> {
    queries::validate_ident(table)?;
    Ok(InsertQuery::new(table))
}

/// Start an UPDATE `table` query.
pub fn update(table: &str) -> Result<UpdateQuery, DbError> {
    queries::validate_ident(table)?;
    Ok(UpdateQuery::new(table))
}

/// Start a DELETE FROM `table` query.
pub fn delete(table: &str) -> Result<DeleteQuery, DbError> {
    queries::validate_ident(table)?;
    Ok(DeleteQuery::new(table))
}


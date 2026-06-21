mod argvalue;
mod commons;
mod executor;
mod interfaces;
mod placeholders;
pub(crate) mod queries;
mod scopes;

pub mod mock;

pub use argvalue::ArgValue;
pub use commons::{Arguments, Database, Pool, QueryResult, Row};
pub use executor::*;
pub use interfaces::{Bindable, Model, Scannable};
pub use queries::{DeleteQuery, InsertQuery, SelectQuery, UpdateQuery};
pub use queries::{FilteredBuilder, LockMode, Page, QueryError, Statement};
pub use scopes::Scope;
pub use sqlx::test as test_db;
pub use vyuh_macros::{Bindable, Scannable};

/// Start a SELECT query against `table`.
pub fn select(table: &str) -> SelectQuery {
    SelectQuery::new(table)
}

/// Start an INSERT INTO `table` query.
pub fn insert(table: &str) -> InsertQuery {
    InsertQuery::new(table)
}

/// Start an UPDATE `table` query.
pub fn update(table: &str) -> UpdateQuery {
    UpdateQuery::new(table)
}

/// Start a DELETE FROM `table` query.
pub fn delete(table: &str) -> DeleteQuery {
    DeleteQuery::new(table)
}

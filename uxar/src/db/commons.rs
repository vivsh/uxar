//! Common database type definitions for all backends.
//!
//! This module provides database and argument type aliases based on the active feature flag.
//! Only one database backend should be enabled at a time.

use crate::db::Filterable;

// PostgreSQL types (default)
#[cfg(feature = "postgres")]
pub type Database = sqlx::Postgres;
#[cfg(feature = "postgres")]
pub type Arguments<'q> = sqlx::postgres::PgArguments;
#[cfg(feature = "postgres")]
pub type Row = sqlx::postgres::PgRow;
#[cfg(feature = "postgres")]
pub type QueryResult = sqlx::postgres::PgQueryResult;
#[cfg(feature = "postgres")]
pub type Pool = sqlx::PgPool;

// MySQL types
#[cfg(feature = "mysql")]
pub type Database = sqlx::MySql;
#[cfg(feature = "mysql")]
pub type Arguments<'q> = sqlx::mysql::MySqlArguments;
#[cfg(feature = "mysql")]
pub type Row = sqlx::mysql::MySqlRow;
#[cfg(feature = "mysql")]
pub type Pool = sqlx::MySqlPool;
#[cfg(feature = "mysql")]
pub type QueryResult = sqlx::mysql::MySqlQueryResult;

// SQLite types
#[cfg(feature = "sqlite")]
pub type Database = sqlx::Sqlite;
#[cfg(feature = "sqlite")]
pub type Arguments<'q> = sqlx::sqlite::SqliteArguments<'q>;
#[cfg(feature = "sqlite")]
pub type Row = sqlx::sqlite::SqliteRow;
#[cfg(feature = "sqlite")]
pub type Pool = sqlx::SqlitePool;
#[cfg(feature = "sqlite")]
pub type QueryResult = sqlx::sqlite::SqliteQueryResult;

// Compile-time check: ensure exactly one database backend is enabled
#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
compile_error!("At least one database backend must be enabled (postgres, mysql, or sqlite)");

#[cfg(all(feature = "postgres", feature = "mysql"))]
compile_error!("Cannot enable both postgres and mysql features simultaneously");

#[cfg(all(feature = "postgres", feature = "sqlite"))]
compile_error!("Cannot enable both postgres and sqlite features simultaneously");

#[cfg(all(feature = "mysql", feature = "sqlite"))]
compile_error!("Cannot enable both mysql and sqlite features simultaneously");

//! Common database type definitions for all backends.
//!
//! This module provides database and argument type aliases based on the active backend feature.
//! Exactly one database backend must be enabled at a time.

#[cfg(not(any(feature = "postgres", feature = "mysql", feature = "sqlite")))]
compile_error!("enable exactly one database backend feature: `postgres`, `mysql`, or `sqlite`");

#[cfg(all(feature = "postgres", feature = "mysql"))]
compile_error!(
    "database backend features are mutually exclusive: disable either `postgres` or `mysql`"
);

#[cfg(all(feature = "postgres", feature = "sqlite"))]
compile_error!(
    "database backend features are mutually exclusive: disable either `postgres` or `sqlite`"
);

#[cfg(all(feature = "mysql", feature = "sqlite"))]
compile_error!(
    "database backend features are mutually exclusive: disable either `mysql` or `sqlite`"
);

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
#[cfg(all(feature = "mysql", not(feature = "postgres")))]
pub type Database = sqlx::MySql;
#[cfg(all(feature = "mysql", not(feature = "postgres")))]
pub type Arguments<'q> = sqlx::mysql::MySqlArguments;
#[cfg(all(feature = "mysql", not(feature = "postgres")))]
pub type Row = sqlx::mysql::MySqlRow;
#[cfg(all(feature = "mysql", not(feature = "postgres")))]
pub type Pool = sqlx::MySqlPool;
#[cfg(all(feature = "mysql", not(feature = "postgres")))]
pub type QueryResult = sqlx::mysql::MySqlQueryResult;

// SQLite types
#[cfg(all(feature = "sqlite", not(any(feature = "postgres", feature = "mysql"))))]
pub type Database = sqlx::Sqlite;
#[cfg(all(feature = "sqlite", not(any(feature = "postgres", feature = "mysql"))))]
pub type Arguments<'q> = sqlx::sqlite::SqliteArguments<'q>;
#[cfg(all(feature = "sqlite", not(any(feature = "postgres", feature = "mysql"))))]
pub type Row = sqlx::sqlite::SqliteRow;
#[cfg(all(feature = "sqlite", not(any(feature = "postgres", feature = "mysql"))))]
pub type Pool = sqlx::SqlitePool;
#[cfg(all(feature = "sqlite", not(any(feature = "postgres", feature = "mysql"))))]
pub type QueryResult = sqlx::sqlite::SqliteQueryResult;

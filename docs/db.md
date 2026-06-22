# Database

Vyuh's database subsystem is a thin SQLx-backed layer around database pools,
sessions, query builders, typed row scanning, typed value binding, and database
error mapping.

The default release posture is Postgres-first. MySQL and SQLite are supported by
the core query-builder and session APIs where SQLx can express the same behavior.
Postgres-only features such as `LISTEN`/`NOTIFY`, row locking, and `RETURNING *`
helpers are gated by the `postgres` feature. Durable task storage is available
for Postgres, MySQL, and SQLite, with Postgres recommended for multi-worker
deployments.

## Overview

The main public pieces are:

- `DbConf` and `DbPool` for SQLx pool setup.
- `DBSession` for code that can run against either a pool or transaction.
- `db::select`, `db::insert`, `db::update`, and `db::delete` for SQL-shaped
  query builders.
- `Statement` for direct SQL when the builders are not the right fit.
- `Scannable` and `Bindable` for structs used with builders.
- `DbError` for database error normalization into framework errors and HTTP
  responses.

Query builders intentionally stay close to SQL. They do not hide table names,
joins, filters, or ordering behind an ORM. SQL fragments remain visible, while
bindings are still passed through SQLx arguments.

## Direct SQLx Access

Vyuh does not replace SQLx. It keeps SQLx as the database foundation and exposes
the underlying pool when direct SQLx is the better tool.

Use `DbPool::as_sqlx()` to reach the active SQLx pool:

```rust
use sqlx::Row as _;
use vyuh::db::DbPool;

# async fn load_count(pool: &DbPool) -> Result<i64, vyuh::db::DbError> {
let row = sqlx::query("SELECT COUNT(*) AS total FROM notes")
    .fetch_one(pool.as_sqlx())
    .await?;
let total: i64 = row.try_get("total")?;
# Ok(total)
# }
```

Use direct SQLx for complex joins, backend-specific SQL, SQLx macros, streaming,
custom JSON aggregation, and queries where the builder would hide more than it
helps. Use Vyuh builders when you want portable named placeholders, typed
`Scannable`/`Bindable` structs, and code that can run against `DBSession`
implementations such as `DbPool`, transactions, or mocks.

## Backend Features

Exactly one database backend feature must be enabled:

```toml
[dependencies]
vyuh = { version = "0.1", default-features = false, features = ["postgres"] }
```

Available backend features are:

- `postgres` - default; enables Postgres SQLx types and Postgres-only helpers.
- `mysql` - enables MySQL SQLx types for the common query/session surface.
- `sqlite` - enables SQLite SQLx types for the common query/session surface.

Compile-time checks reject builds with no backend or multiple backend features.

## Configuration

`DbConf` can be built directly, loaded from `DATABASE_URL`, or parsed from a URL
with pool settings in the query string:

```rust
use vyuh::db::{DbConf, DbPool};

# async fn build_pool() -> Result<(), vyuh::db::DbError> {
let conf = DbConf::from_url("postgres://localhost/app?max=20&min=2&lazy=true")?;
let pool = DbPool::from_conf(&conf).await?;
# Ok(())
# }
```

The supported URL options are:

- `max` - maximum pool connections.
- `min` - minimum pool connections.
- `lazy` - whether SQLx should connect lazily.

`Site::db()` returns the site-scoped `DbPool`.

## Macro Sugar And Direct Traits

Database derive macros are sugar over direct trait implementations:

- `#[derive(Scannable)]` implements `db::Scannable` and `sqlx::FromRow`.
- `#[derive(Bindable)]` implements `db::Bindable`.

The same behavior can be written manually by implementing those traits directly.
Use the derives for ordinary structs and direct trait implementations when a
type needs custom column ordering, nested scanning, or binding behavior.

```rust
#[derive(Debug, Clone, vyuh::db::Scannable)]
struct Note {
    id: i64,
    title: String,
    done: bool,
}
```

The generated `Scannable::scan_column_names()` drives the selected columns for
`db::select("notes").all::<Note, _>(...)`.

## Query Builders

Query builders are created through functions, not macros:

```rust
use vyuh::db::{self, FilteredBuilder};

# async fn load<S: vyuh::db::DBSession>(session: &mut S) -> Result<(), vyuh::db::DbError> {
# #[derive(Debug, Clone, vyuh::db::Scannable)]
# struct Note { id: i64, title: String, done: bool }
let notes: Vec<Note> = db::select("notes")
    .filter("done = :done")
    .bind_as("done", false)
    .order_by("id", true)
    .all(session)
    .await?;
# Ok(())
# }
```

Builder errors, bind errors, placeholder errors, and invalid identifiers are
stored in the builder and returned by the terminal async call.

The common terminal methods are:

- `execute` for write queries.
- `one` for exactly one row.
- `first` for zero-or-one row.
- `all` for all rows.
- `count`, `exists`, and `page` for select queries.

## Query Builder Methods

### Shared Filtering

- `filter(sql)` - Adds a raw SQL predicate joined with `AND`.
- `bind(value)` - Adds a positional SQLx bind value.
- `bind_as(name, value)` - Adds a named bind value used by `:name` placeholders.

### `db::select(table)`

- `alias(prefix, alias)` - Maps dotted scan-column prefixes to table aliases.
- `group_by(column)` - Adds a `GROUP BY` column.
- `having(sql)` - Adds a raw `HAVING` predicate joined with `AND`.
- `order_by(column, ascending)` - Adds an `ORDER BY` expression.
- `paginate(page, per_page)` - Sets one-indexed page pagination.
- `slice(offset, count)` - Sets `LIMIT` and `OFFSET` directly.
- `select_expr(name, scope)` - Supplies a computed expression for a scanned column.
- `for_update()` - Adds `FOR UPDATE` on Postgres.
- `for_share()` - Adds `FOR SHARE` on Postgres.
- `one(session)` - Fetches exactly one typed row.
- `first(session)` - Fetches an optional typed row.
- `all(session)` - Fetches all typed rows.
- `count(session)` - Fetches the count for the filtered query.
- `exists(session)` - Fetches whether any filtered row exists.
- `page(session)` - Fetches rows plus pagination metadata.

### `db::insert(table)`

- `row(item)` - Binds one `Bindable` item for insertion.
- `rows(items)` - Binds multiple `Bindable` items for bulk insertion.
- `upsert(item, conflict_cols)` - Inserts or does nothing on Postgres conflict.
- `upsert_update(item, conflict_cols)` - Inserts or updates non-conflict columns on Postgres conflict.
- `execute(session)` - Executes the insert and returns affected rows.
- `one(session)` - Inserts and returns one row via Postgres `RETURNING *`.
- `first(session)` - Inserts and returns an optional row via Postgres `RETURNING *`.
- `all(session)` - Inserts and returns all rows via Postgres `RETURNING *`.

### `db::update(table)`

- `set(item)` - Builds the `SET` clause from a `Bindable` item.
- `execute(session)` - Executes the update and returns affected rows.
- `one(session)` - Updates and returns one row via Postgres `RETURNING *`.
- `first(session)` - Updates and returns an optional row via Postgres `RETURNING *`.
- `all(session)` - Updates and returns all rows via Postgres `RETURNING *`.

### `db::delete(table)`

- `execute(session)` - Executes the delete and returns affected rows.
- `first(session)` - Deletes and returns an optional row via Postgres `RETURNING *`.
- `all(session)` - Deletes and returns all rows via Postgres `RETURNING *`.

## Named Placeholders

Vyuh supports named placeholders in builder SQL fragments:

```rust
use vyuh::db::{self, FilteredBuilder};

# async fn count_open<S: vyuh::db::DBSession>(session: &mut S) -> Result<i64, vyuh::db::DbError> {
let total = db::select("notes")
    .filter("done = :done")
    .bind_as("done", false)
    .count(session)
    .await?;
# Ok(total)
# }
```

Named placeholders are resolved to the active backend's placeholder syntax at
execution time. Extra named bindings are ignored only when they are not required
by the SQL; missing placeholders return a `QueryError`.

## Inserts And Updates

`Bindable` controls which struct fields are written:

```rust
use vyuh::db::{self, FilteredBuilder};

#[derive(Debug, vyuh::db::Bindable)]
struct NotePatch {
    done: bool,
}

# async fn mark_done<S: vyuh::db::DBSession>(session: &mut S) -> Result<(), vyuh::db::DbError> {
let patch = NotePatch { done: true };
db::update("notes")
    .set(&patch)
    .filter("id = :id")
    .bind_as("id", 1_i64)
    .execute(session)
    .await?;
# Ok(())
# }
```

Postgres builds also expose `RETURNING *` helpers for `insert`, `update`, and
`delete`.

## Direct Statements

Use `Statement` when hand-written SQL is clearer than a builder but you still
want to execute through the `DBSession` abstraction:

```rust
use vyuh::db::{DBSession, Statement};

# async fn count<S: DBSession>(session: &mut S) -> Result<i64, vyuh::db::DbError> {
let total: i64 = session
    .fetch_scalar(Statement::from_str("SELECT COUNT(*) FROM notes WHERE done = $1").bind(false))
    .await?;
# Ok(total)
# }
```

`Statement` is intentionally low-level. Placeholder syntax in raw SQL is the
database driver's syntax, not Vyuh's named-placeholder syntax.

## Sessions And Transactions

Query code should usually accept `impl DBSession`. That lets the same function
run against a `DbPool`, a transaction, or the mock DB session used in tests.

```rust
use vyuh::db::{self, DBSession};

# #[derive(Debug, vyuh::db::Bindable)]
# struct NewTodo { title: String }
async fn create_todo<S: DBSession>(session: &mut S, title: String) -> Result<u64, vyuh::db::DbError> {
    db::insert("todos")
        .row(&NewTodo { title })
        .execute(session)
        .await
}
```

Transactions are started from `DbPool::begin()` and implement `DBSession`.

## Mock Sessions

`vyuh::db::mock::MockDBSession` records SQL and returns planned responses. It is
useful for testing query construction without a live database.

```rust
use vyuh::db;
use vyuh::db::mock::MockDBSession;

# async fn test_query() -> Result<(), vyuh::db::DbError> {
let mut db = MockDBSession::new();
db.plan_fetch_scalar_ok("COUNT(*)", 2_i64);

let total = db::select("notes").count(&mut db).await?;
assert_eq!(total, 2);
# Ok(())
# }
```

## Examples

- [`db_basic.rs`](../vyuh/examples/db/basic.rs): select rows with a typed
  `Scannable` result and named filters.
- [`db_writes.rs`](../vyuh/examples/db/writes.rs): insert and update rows with
  `Bindable` structs.
- [`db_raw_statement.rs`](../vyuh/examples/db/raw_statement.rs): execute direct
  SQL through `Statement`.
- [`db_sqlx.rs`](../vyuh/examples/db/sqlx.rs): use direct SQLx against the
  underlying `DbPool`.
- [`db_transactions.rs`](../vyuh/examples/db/transactions.rs): run builders
  inside a transaction.

## Failure Modes

- Invalid table/source identifiers return `QueryError::InvalidIdentifier`.
- Missing row data for `insert` or `update` returns a bind error.
- Empty bulk inserts are rejected.
- Missing named placeholder values return a placeholder error.
- SQLx row-not-found errors map to `DbError::DoesNotExist`.
- SQLx database constraint errors map to `DbError::Integrity`.
- Backend-specific helpers return `DbError::Unsupported` when unavailable.

## Current Limitations

- Vyuh does not provide migrations or schema management in v0.
- DB derives do not form a full ORM; joins and relationship loading remain
  explicit SQL/query-builder work.
- Raw `Statement` SQL uses native SQLx placeholder syntax.
- Postgres-only helpers are intentionally not emulated on MySQL or SQLite.

# Dependency Architecture Improvements

## Problem

Previously, the proc macros (`Schemable`, `Scannable`, `Bindable`) directly referenced external crates like `sqlx` and `serde`:

```rust
// Old approach - macros directly reference sqlx
impl Scannable for User {
    fn scan_row(row: &::sqlx::postgres::PgRow) -> Result<Self, ::sqlx::Error> {
        // ...
    }
}
```

This caused issues:
- Macros tightly coupled to specific versions of external crates
- Users needed to have matching versions of sqlx in their dependencies
- No central control over dependency versions

## Solution

Re-export all external types through `uxar::db`:

```rust
// uxar/src/db/mod.rs
pub use sqlx;
pub use sqlx::{
    Error as SqlxError,
    Row,
    FromRow,
    postgres::{PgRow, PgArguments, Postgres},
};
pub use serde;
pub use serde_json;
```

Macros now reference uxar's re-exports:

```rust
// New approach - macros reference uxar's re-exports
impl Scannable for User {
    fn scan_row(row: &::uxar::db::PgRow) -> Result<Self, ::uxar::db::SqlxError> {
        // ...
    }
}
```

## Benefits

1. **Decoupling**: Macros only depend on uxar's public API, not external crates directly
2. **Version control**: uxar controls sqlx/serde versions; users automatically get compatible versions
3. **Cleaner dependencies**: `uxar-macros` crate doesn't need sqlx in Cargo.toml
4. **Consistent namespace**: Everything goes through `uxar::db::*`
5. **Flexibility**: Can swap out implementations or add compatibility layers without changing macro code

## Changes Made

### Updated Files

1. **uxar/src/db/mod.rs**: Added re-exports of sqlx, serde, and serde_json
2. **uxar/src/db/interfaces.rs**: Updated trait definitions to use re-exported types
3. **uxar-macros/src/bindable.rs**: Changed all `::sqlx` references to `::uxar::db::sqlx`
4. **uxar-macros/src/scannable.rs**: Changed all `::sqlx` references to `::uxar::db::sqlx`

### Type Mappings

| Old Path | New Path |
|----------|----------|
| `::sqlx::Error` | `::uxar::db::SqlxError` |
| `::sqlx::postgres::PgRow` | `::uxar::db::PgRow` |
| `::sqlx::postgres::PgArguments` | `::uxar::db::PgArguments` |
| `::sqlx::Postgres` | `::uxar::db::Postgres` |
| `::sqlx::FromRow` | `::uxar::db::FromRow` |
| `::sqlx::Row` | `::uxar::db::Row` |
| `::serde::Serialize` | `::uxar::db::serde::Serialize` |
| `::serde_json::Value` | `::uxar::db::serde_json::Value` |

## Pattern to Follow

When adding new macros or traits that reference external dependencies:

1. **Re-export** the types in the appropriate uxar module
2. **Reference** through uxar's namespace in macro code
3. **Document** the re-exports clearly

This pattern is common in the Rust ecosystem (e.g., Diesel, Actix-web) and provides better API stability.

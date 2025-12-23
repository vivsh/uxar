# uxar-macros Best Practices Guide

## Overview
This guide covers production-grade usage of all uxar-macros derive macros with clear error messages and strict validation.

## Table of Contents
- [Validatable](#validatable)
- [Schemable](#schemable)
- [Bindable & Scannable](#bindable--scannable)
- [Filterable](#filterable)
- [Routable](#routable)
- [Common Pitfalls](#common-pitfalls)

---

## Validatable

### Purpose
Derives the `Validate` trait for struct field validation with comprehensive error reporting.

### Available Validators

#### String Validators
- `email` - Valid email format
- `url` - Valid URL format
- `uuid` - Valid UUID format
- `non_empty` - Non-empty string
- `alphanumeric` - Only letters and numbers
- `digits` - Only digits
- `min_length = N` - Minimum string length
- `max_length = N` - Maximum string length
- `regex = "pattern"` - Custom regex pattern

#### Numeric Validators
- `min_value = "N"` - Minimum numeric value
- `max_value = "N"` - Maximum numeric value

#### Special Validators
- `required` - For `Option<T>`, ensures value is `Some`
- `nested` - Validates nested struct that implements `Validate`
- `skip` - Skips validation for this field
- `custom = "fn_name"` - Custom validation function

### Rules and Restrictions

1. **Format validators are mutually exclusive**: Cannot use `email`, `url`, `uuid`, `alphanumeric`, or `digits` together.
   ```rust
   // ❌ ERROR
   #[validate(email, url)]
   
   // ✅ CORRECT
   #[validate(email)]
   ```

2. **Type compatibility**: String validators only work on `String`/`&str`, numeric validators only on numeric types.
   ```rust
   // ❌ ERROR - email on number
   #[validate(email)]
   age: i32,
   
   // ✅ CORRECT
   #[validate(min_value = "0")]
   age: i32,
   ```

3. **Nested conflicts**: Cannot use `nested` with scalar validators.
   ```rust
   // ❌ ERROR
   #[validate(nested, email)]
   
   // ✅ CORRECT - nested only
   #[validate(nested)]
   ```

4. **Skip conflicts**: Cannot use `skip` with any other validators.
   ```rust
   // ❌ ERROR
   #[validate(skip, required)]
   
   // ✅ CORRECT
   #[validate(skip)]
   ```

5. **Regex validation**: Patterns are validated at compile time for basic syntax errors.
   ```rust
   // ❌ ERROR - unbalanced brackets
   #[validate(regex = "[abc")]
   
   // ✅ CORRECT
   #[validate(regex = "^[abc]+$")]
   ```

6. **Min/Max logic**: `min_length` cannot exceed `max_length`.
   ```rust
   // ❌ ERROR
   #[validate(min_length = 10, max_length = 5)]
   
   // ✅ CORRECT
   #[validate(min_length = 5, max_length = 10)]
   ```

### Best Practices

```rust
use uxar::Validatable;

#[derive(Validatable)]
struct UserInput {
    // Combine compatible validators
    #[validate(email, non_empty)]
    email: String,
    
    // Multiple constraints on numbers
    #[validate(min_value = "18", max_value = "120")]
    age: i32,
    
    // Optional fields with validation
    #[validate(url)]
    website: Option<String>,
    
    // Nested validation
    #[validate(nested)]
    address: Address,
}
```

---

## Schemable

### Purpose
Generates database schema metadata for query building and ORM operations.

### Available Attributes

- `skip` - Exclude field from schema entirely
- `flatten` - Embed nested struct's columns
- `reference` - Mark as relationship (tracked but not in flat schema)
- `json` - Store as JSONB column
- `db_column = "name"` - Override column name
- `selectable = bool` - Include in SELECT queries
- `insertable = bool` - Include in INSERT queries
- `updatable = bool` - Include in UPDATE queries

### Rules and Restrictions

1. **Column kinds are mutually exclusive**: Only use one of `flatten`, `reference`, or `json`.
   ```rust
   // ❌ ERROR
   #[column(flatten, reference)]
   
   // ✅ CORRECT - choose one
   #[column(flatten)]
   ```

2. **Skip conflicts**: Cannot combine `skip` with any configuration.
   ```rust
   // ❌ ERROR
   #[column(skip, db_column = "custom")]
   
   // ✅ CORRECT
   #[column(skip)]
   ```

3. **Validation on special columns**: Cannot use validation attributes on `flatten` or `reference` columns.
   ```rust
   // ❌ ERROR
   #[column(flatten)]
   #[validate(email)]
   
   // ✅ CORRECT - validate the nested type instead
   #[column(flatten)]
   ```

### Best Practices

```rust
use uxar::db::Schemable;

#[derive(Schemable)]
struct User {
    // Auto-generated, not insertable
    #[column(insertable = false)]
    id: i32,
    
    // Custom database column name
    #[column(db_column = "user_name")]
    name: String,
    
    // JSON storage
    #[column(json)]
    metadata: serde_json::Value,
    
    // Flatten audit fields
    #[column(flatten)]
    audit: AuditFields,
    
    // Skip runtime-only field
    #[column(skip)]
    cached_value: String,
}
```

---

## Bindable & Scannable

### Purpose
- **Bindable**: Serialize struct fields to SQL query parameters
- **Scannable**: Deserialize SQL result rows to struct fields

### Notes
- Use same `#[column(...)]` attributes as `Schemable`
- Fields marked `skip` use `Default::default()` for Scannable
- `flatten` recursively delegates to nested type
- `json` uses JSON serialization/deserialization

### Error Messages
Now include field names and column indices for debugging:
```
Field 'user_name' (column index 3): Failed to scan - type mismatch
```

---

## Filterable

### Purpose
Generates query filtering logic from struct fields.

### Available Attributes

- `skip` - Exclude from filtering
- `delegate` - Delegate to nested `Filterable` type
- `db_column = "name"` - Override column name
- `expr = "SQL"` - Use custom SQL expression (⚠️ **SQL injection risk**)
- `op = "operator"` - SQL comparison operator

### Supported Operators

**Comparison**: `=`, `!=`, `<>`, `<`, `>`, `<=`, `>=`  
**Pattern**: `LIKE`, `ILIKE`, `NOT LIKE`, `NOT ILIKE`  
**Set**: `IN`, `NOT IN`  
**Null**: `IS`, `IS NOT`  
**PostgreSQL Arrays**: `@>`, `<@`, `&&`  
**PostgreSQL Regex**: `~`, `~*`, `!~`, `!~*`

### Rules and Restrictions

1. **Operator validation**: Only whitelisted operators allowed.
   ```rust
   // ❌ ERROR
   #[filter(op = "INVALID")]
   
   // ✅ CORRECT
   #[filter(op = "ILIKE")]
   ```

2. **Delegate conflicts**: Cannot combine `delegate` with scalar config.
   ```rust
   // ❌ ERROR
   #[filter(delegate, op = "=")]
   
   // ✅ CORRECT
   #[filter(delegate)]
   ```

3. **Skip conflicts**: Cannot combine `skip` with other attributes.

### Best Practices

```rust
use uxar::db::Filterable;

#[derive(Filterable)]
struct UserFilter {
    // Exact match
    id: Option<i32>,
    
    // Case-insensitive search
    #[filter(op = "ILIKE")]
    email: Option<String>,
    
    // Range queries
    #[filter(op = ">=")]
    min_age: Option<i32>,
    
    // Custom expressions (use with caution!)
    #[filter(expr = "LOWER(username)")]
    username_lower: Option<String>,
    
    // Nested filtering
    #[filter(delegate)]
    address: Option<AddressFilter>,
}
```

**⚠️ Security Warning**: The `expr` attribute allows arbitrary SQL. Always sanitize or use only with trusted, hardcoded values.

---

## Routable

### Purpose
Generates Axum route registration and OpenAPI metadata.

### Attributes

#### `#[routable(...)]` (impl block)
- `base_path = "/path"` - Prefix for all routes

#### `#[route(...)]` (method)
- `name = "custom"` - Logical name (default: method name)
- `method = "HTTP_METHOD"` or `method = ["GET", "POST"]` - HTTP methods
- `url = "/path"` - Route path (default: `/{name}`)
- `summary = "text"` - API documentation
- `param(name = "x", ...)` - Parameter overrides
- `response(status = 200, ...)` - Response overrides

### Supported HTTP Methods
`GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `HEAD`, `OPTIONS`

### Rules and Restrictions

1. **Valid HTTP methods**: Only supported methods allowed.
   ```rust
   // ❌ ERROR
   #[route(method = "INVALID")]
   
   // ✅ CORRECT
   #[route(method = "POST")]
   ```

2. **HEAD restrictions**: Cannot accept body extractors.
   ```rust
   // ❌ ERROR
   #[route(method = "HEAD")]
   async fn handler(Json(body): Json<T>) {}
   
   // ✅ CORRECT
   #[route(method = "HEAD")]
   async fn handler() {}
   ```

3. **Path format**: Must start with `/`, no double slashes, no trailing slash (except `/`).
   ```rust
   // ❌ ERROR
   #[route(url = "no-slash")]
   #[route(url = "/api//users")]
   #[route(url = "/api/users/")]
   
   // ✅ CORRECT
   #[route(url = "/api/users")]
   ```

### Best Practices

```rust
use uxar::{routable, route};
use axum::{Json, extract::Path};

struct ApiView;

#[routable(base_path = "/api")]
impl ApiView {
    /// List resources
    #[route(summary = "List all users")]
    async fn index() -> Json<Vec<User>> {
        Json(vec![])
    }
    
    /// Get by ID
    #[route(url = "/{id}", summary = "Get user by ID")]
    async fn show(Path(id): Path<i32>) -> Json<User> {
        // ...
    }
    
    /// Create resource
    #[route(method = "POST", summary = "Create user")]
    async fn create(Json(req): Json<CreateReq>) -> Json<User> {
        // ...
    }
    
    /// Multiple methods
    #[route(method = ["GET", "POST"], url = "/health")]
    async fn health() -> &'static str {
        "OK"
    }
}
```

---

## Common Pitfalls

### 1. Typos in Attributes
**Problem**: Misspelled attributes are silently ignored.
```rust
// ❌ Typo - silently ignored!
#[column(jsoon = true)]
```
**Solution**: Always test your code and check generated output.

### 2. Over-nesting
**Problem**: Too many nested `Validatable` structs.
```rust
// Consider flattening or simplifying
```

### 3. SQL Injection via `expr`
**Problem**: User input in SQL expressions.
```rust
// ❌ DANGEROUS
#[filter(expr = user_provided_column)]
```
**Solution**: Only use `expr` with hardcoded, trusted values.

### 4. Missing Trait Bounds
**Problem**: Generic types may need explicit bounds.
```rust
#[derive(Schemable)]
struct Generic<T> {
    data: T, // May need: where T: SomeTraits
}
```

### 5. Assuming Default Behavior
**Problem**: Not understanding defaults (e.g., `selectable = true`).  
**Solution**: Be explicit when behavior matters:
```rust
#[column(insertable = false, updatable = false)]
id: i32,
```

---

## Error Message Guide

All macros now provide detailed, actionable error messages:

### Example 1: Conflicting Attributes
```
error: Field 'email': Cannot use multiple format validators: email, url. Choose one.
  --> src/main.rs:5:5
   |
5  |     #[validate(email, url)]
   |     ^^^^^^^^^^^^^^^^^^^^^^^
```

### Example 2: Type Mismatch
```
error: Field 'age': String validators (email, url, ...) can only be used on String or &str types. Found: i32
  --> src/main.rs:8:5
   |
8  |     #[validate(email)]
   |     ^^^^^^^^^^^^^^^^^^
```

### Example 3: Runtime Errors (Improved)
```
Field 'preferences' (column index 5): Failed to deserialize JSON - missing field `theme`
```

---

## Testing Your Macros

Use compile-fail tests to verify error messages:

```rust
#[test]
fn test_compile_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
```

---

## Summary

- ✅ All macros now validate attributes at compile time
- ✅ Clear, actionable error messages with precise spans
- ✅ Type-safe: validators match field types
- ✅ Secure: SQL operators whitelisted, injection warnings
- ✅ Well-tested: Comprehensive test coverage

For more examples, see `uxar/examples/macro_usage_guide.rs`.

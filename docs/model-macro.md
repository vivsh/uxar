# Model Derive Macro

The `Model` derive macro combines four essential database-related derive macros into one convenient annotation:

- **Schemable**: Generates schema metadata including column names, types, and validation rules
- **Scannable**: Implements row scanning from SQL query results 
- **Bindable**: Implements parameter binding for SQL queries
- **Validatable**: Implements validation logic based on `#[validate(...)]` attributes

## Usage

```rust
use uxar::db::Model;

#[derive(Model)]
#[model(crate = "uxar::db")]
struct User {
    #[column(db_column = "user_id")]
    id: i32,

    #[column(db_column = "user_email")]
    #[validate(email)]
    email: String,

    #[validate(min_length = 3, max_length = 50)]
    username: String,

    #[validate(range = (18, 120))]
    age: i32,
}
```

## Benefits

### Without Model
```rust
#[derive(Schemable, Scannable, Bindable, Validatable)]
#[schemable(crate = "uxar::db")]
struct User {
    // fields...
}
```

### With Model  
```rust
#[derive(Model)]
#[model(crate = "uxar::db")]
struct User {
    // fields...
}
```

**Advantages:**
- ✅ Less boilerplate - one derive instead of four
- ✅ Ensures all database traits are implemented consistently
- ✅ Single `#[model]` attribute instead of `#[schemable]`
- ✅ Same field-level attributes (`#[column]`, `#[validate]`)
- ✅ More ergonomic for the common case of database models

## Attributes

### Container Attributes

- `#[model(crate = "path::to::uxar::db")]` - Specifies the crate path (defaults to `uxar::db`)
- `#[model(name = "custom_table_name")]` - Override the table/schema name

### Field Attributes

#### Column Attributes (`#[column(...)]`)

- `db_column = "column_name"` - Override the database column name
- `skip` - Skip this field in database operations
- `flatten` - Flatten nested struct fields
- `json` - Store field as JSON in database

#### Validation Attributes (`#[validate(...)]`)

- `email` - Validates email format
- `url` - Validates URL format
- `uuid` - Validates UUID format
- `ipv4` - Validates IPv4 address
- `min_length = N` - Minimum string length
- `max_length = N` - Maximum string length
- `exact_length = N` - Exact string length
- `min_value = N` - Minimum numeric value
- `max_value = N` - Maximum numeric value
- `range = (min, max)` - Numeric range validation
- `regex = "pattern"` - Custom regex pattern
- `alphanumeric` - Only alphanumeric characters
- `slug` - Valid URL slug format
- `digits` - Only digit characters
- `non_empty` - String cannot be empty

## Implementation Details

The `Model` macro internally calls all four derive implementations:
1. Parses the struct with all attributes
2. Transforms `#[model(...)]` to `#[schemable(...)]` for schema generation
3. Generates all four trait implementations in a single pass

This approach is more efficient than running four separate macro expansions and ensures consistency across all implementations.

# Validatable Derive Macro

## Overview

The `Validatable` derive macro automatically generates `Validate` trait implementations for structs, providing declarative field-level validation using attributes.

## Features

- ✅ Declarative validation via `#[validate(...)]` attributes
- ✅ Handles both required (`T`) and optional (`Option<T>`) fields automatically
- ✅ Comprehensive validator set: email, URL, UUID, regex, numeric ranges, string lengths, etc.
- ✅ Nested struct validation with `#[validate(nested)]`
- ✅ Custom validator functions
- ✅ Skip fields with `#[validate(skip)]`
- ✅ Zero panics - all validation is safe
- ✅ Modular design - `generate_field_validation()` can be reused for schema generation

## Supported Validators

### String Validators
- `email` - Validates email address format
- `url` - Validates URL format (http/https)
- `alphanumeric` - Only letters and numbers
- `digits` - Only numeric digits
- `slug` - URL-friendly slug format
- `uuid` - UUID format validation
- `ipv4` - IPv4 address format
- `non_empty` - Non-empty string
- `min_length = N` - Minimum byte length
- `max_length = N` - Maximum byte length
- `exact_len = N` - Exact byte length
- `min_chars = N` - Minimum character count (Unicode-aware)
- `max_chars = N` - Maximum character count (Unicode-aware)
- `regex = "pattern"` - Custom regex pattern

### Numeric Validators
- `min_value = "N"` - Minimum value (for any `PartialOrd` type)
- `max_value = "N"` - Maximum value (for any `PartialOrd` type)

### Collection Validators
- `non_empty` - Works on `Vec<T>` and other collections
- `min_items = N` - Minimum number of items
- `max_items = N` - Maximum number of items

### Option Validators
- `required` - Ensures `Option<T>` contains `Some`

### Structural Validators
- `nested` - Validates nested struct (must impl `Validate`)
- `custom = "fn_path"` - Call custom validator function
- `skip` - Skip validation for this field

## Usage Examples

### Basic Example

```rust
use uxar::{validation::Validate, Validatable};

#[derive(Validatable)]
struct CreateUser {
    #[validate(email, non_empty)]
    email: String,

    #[validate(min_length = 8, max_length = 100)]
    password: String,

    #[validate(min_value = "18", max_value = "120")]
    age: i32,
}

let user = CreateUser {
    email: "user@example.com".to_string(),
    password: "secure123".to_string(),
    age: 25,
};

match user.validate() {
    Ok(()) => println!("Valid!"),
    Err(report) => {
        // report contains all validation errors with field paths
        let errors = report.into_field_map_flat();
        for (field, messages) in errors {
            for msg in messages {
                println!("{}: {}", field, msg);
            }
        }
    }
}
```

### Optional Fields

Optional fields are automatically handled - validators only run if `Some`:

```rust
#[derive(Validatable)]
struct UserProfile {
    #[validate(email)]
    email: String,

    // URL validation only runs if Some
    #[validate(url)]
    website: Option<String>,

    // Ensures Option contains Some
    #[validate(required)]
    bio: Option<String>,
}
```

### Nested Validation

```rust
#[derive(Validatable)]
struct Address {
    #[validate(min_length = 5)]
    street: String,

    #[validate(alphanumeric)]
    city: String,
}

#[derive(Validatable)]
struct User {
    #[validate(email)]
    email: String,

    // Validates nested struct
    #[validate(nested)]
    address: Address,

    // Works with Option too
    #[validate(nested)]
    shipping_address: Option<Address>,
}
```

### Custom Validators

```rust
fn validate_username(s: &str) -> Result<(), ValidationError> {
    if s.starts_with('_') {
        return Err(ValidationError::new(
            "invalid_username",
            "Username cannot start with underscore"
        ));
    }
    Ok(())
}

#[derive(Validatable)]
struct SignupForm {
    #[validate(custom = "validate_username", min_length = 3)]
    username: String,
}
```

### Combining Multiple Validators

Multiple validators can be combined on a single field:

```rust
#[derive(Validatable)]
struct Product {
    // Checks: not empty AND alphanumeric AND length 3-20
    #[validate(non_empty, alphanumeric, min_length = 3, max_length = 20)]
    sku: String,

    // Checks: range 0.01-10000.00
    #[validate(min_value = "0.01", max_value = "10000.00")]
    price: f64,

    // Checks: not empty AND each item 10-200 chars
    #[validate(non_empty, min_length = 10, max_length = 200)]
    descriptions: Vec<String>,
}
```

### Skip Validation

```rust
#[derive(Validatable)]
struct InternalModel {
    #[validate(email)]
    email: String,

    // Not validated
    #[validate(skip)]
    internal_notes: String,

    // Not validated
    #[validate(skip)]
    metadata: serde_json::Value,
}
```

## Error Handling

Validation errors are returned as a `ValidationReport` containing:
- Field paths (supports nested structures)
- Error codes (e.g., `"min_value"`, `"invalid_email"`)
- Human-readable messages
- Parameters (e.g., min/max values)

Convert to different formats:

```rust
let report: ValidationReport = user.validate().unwrap_err();

// Flat map: field -> Vec<message>
let flat: BTreeMap<String, Vec<String>> = report.into_field_map_flat();

// Nested JSON structure
let nested: serde_json::Value = report.to_nested_map();

// Access individual issues
for issue in report.issues {
    println!("{}: {}", issue.path, issue.error.message);
}
```

## Integration with Axum

Use with the `Valid<E>` extractor for automatic request validation:

```rust
use axum::{Json, Router, routing::post};
use uxar::{Valid, Validatable};

#[derive(Validatable, serde::Deserialize)]
struct CreateUser {
    #[validate(email)]
    email: String,

    #[validate(min_length = 8)]
    password: String,
}

async fn create_user(
    // Automatically validates and returns 422 with errors if invalid
    Valid(Json(user)): Valid<Json<CreateUser>>,
) -> &'static str {
    // user is guaranteed to be valid here
    "User created"
}

let app = Router::new()
    .route("/users", post(create_user));
```

## Schema Integration

The macro is designed for reusability with schema generation:

```rust
// In your schema generation code:
use uxar_macros::generate_field_validation;

// Extract validation metadata from fields
let validations = generate_field_validation(
    &field,
    &field_name,
    &crate_path,
);

// Use validation info to generate OpenAPI/JSON Schema constraints
```

## Performance Notes

- All regex patterns are compiled once using `once_cell::sync::Lazy`
- Zero runtime overhead for skipped fields
- No unnecessary allocations
- Iterator-based error collection
- Pre-allocated capacity hints where appropriate

## Design Principles

Following repository guidelines:
- **No panics**: All operations return `Result` or `Option`
- **Functions <50 lines**: Each validator is small and focused
- **Efficient**: Borrows where possible, minimal allocations
- **Descriptive names**: Clear 1-4 word identifiers (email, url, non_empty, min_value)
- **Ergonomic**: Works seamlessly with `Option<T>`
- **Composable**: Multiple validators can be combined

## Examples

See:
- [uxar/tests/validatable.rs](../tests/validatable.rs) - Unit tests
- [uxar/examples/validatable_demo.rs](../examples/validatable_demo.rs) - Comprehensive demo

Run demo:
```bash
cargo run -p uxar --example validatable_demo
```

Run tests:
```bash
cargo test -p uxar --test validatable
```

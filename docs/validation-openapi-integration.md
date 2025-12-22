# Validation & OpenAPI Schema Integration

## Overview

The validation system integrates with OpenAPI schema generation, allowing you to write validation rules once and have them automatically reflected in your API documentation.

## Usage

Use the same `#[validate(...)]` attributes on your structs, and they will be:
1. Enforced at runtime via the `Validate` trait
2. Included in OpenAPI schema constraints for documentation

### Example

```rust
use uxar::{Validatable, Schemable};

#[derive(Validatable, Schemable, serde::Deserialize)]
struct CreateUser {
    // Runtime validation: email format
    // OpenAPI: format: "email"
    #[validate(email, non_empty)]
    email: String,

    // Runtime validation: 8-100 chars
    // OpenAPI: minLength: 8, maxLength: 100
    #[validate(min_length = 8, max_length = 100)]
    password: String,

    // Runtime validation: 3-50 chars
    // OpenAPI: minLength: 3, maxLength: 50
    #[validate(min_length = 3, max_length = 50)]
    username: String,

    // Runtime validation: 18-120
    // OpenAPI: minimum: 18, maximum: 120
    #[validate(min_value = "18", max_value = "120")]
    age: i32,

    // Runtime validation: URL format
    // OpenAPI: format: "uri"
    #[validate(url)]
    website: Option<String>,

    // Runtime validation: UUID format
    // OpenAPI: format: "uuid"
    #[validate(uuid)]
    user_id: Option<String>,

    // Runtime validation: custom regex
    // OpenAPI: pattern: "^[a-z0-9_]+$"
    #[validate(pattern = "^[a-z0-9_]+$")]
    slug: String,
}
```

## Attribute Mapping

| Validation Attribute | OpenAPI Schema Constraint | Notes |
|---------------------|---------------------------|-------|
| `email` | `format: "email"` | RFC 5321 email format |
| `url` | `format: "uri"` | RFC 3986 URI format |
| `uuid` | `format: "uuid"` | RFC 4122 UUID format |
| `min_length = N` | `minLength: N` | String byte length |
| `max_length = N` | `maxLength: N` | String byte length |
| `min_value = "N"` | `minimum: N` | Numeric minimum (inclusive) |
| `max_value = "N"` | `maximum: N` | Numeric maximum (inclusive) |
| `pattern = "regex"` | `pattern: "regex"` | Regular expression |
| `non_empty` | `minLength: 1` | Non-empty string |

## Validators Without OpenAPI Mapping

Some validators are runtime-only and don't map directly to OpenAPI:

- `alphanumeric` - Pattern could be added manually
- `digits` - Pattern could be added manually
- `slug` - Pattern could be added manually  
- `ipv4` - Could use `format: "ipv4"` (OpenAPI 3.1)
- `nested` - Handled via `$ref` in schema composition
- `custom` - Application-specific validation

## Usage with Axum

Combine with `Valid<>` extractor for automatic validation:

```rust
use axum::{Json, Router, routing::post};
use uxar::{Valid, Validatable, Schemable};

#[derive(Validatable, Schemable, serde::Deserialize)]
struct CreateUser {
    #[validate(email, non_empty)]
    email: String,

    #[validate(min_length = 8)]
    password: String,
}

async fn create_user(
    // Validates automatically, returns 422 if invalid
    Valid(Json(user)): Valid<Json<CreateUser>>,
) -> &'static str {
    // user is valid here
    "User created"
}

let app = Router::new()
    .route("/users", post(create_user));
```

The OpenAPI documentation will show:
```yaml
/users:
  post:
    requestBody:
      content:
        application/json:
          schema:
            type: object
            required:
              - email
              - password
            properties:
              email:
                type: string
                format: email
                minLength: 1
              password:
                type: string
                minLength: 8
```

## Implementation Details

### SchemableField Enhancement

The `SchemableField` struct in the macro now includes validation attributes:

```rust
#[derive(FromField)]
#[darling(attributes(column, validate))]
pub(crate) struct SchemableField {
    // ... existing column attributes ...
    
    // Validation attributes
    pub email: bool,
    pub url: bool,
    pub uuid: bool,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub pattern: Option<String>,
    pub non_empty: bool,
}
```

### Constraint Generation

The `field_validation_constraints()` helper converts these attributes to OpenAPI schema methods:

```rust
fn field_validation_constraints(field: &SchemableField) -> TokenStream {
    let mut constraints = Vec::new();
    
    if field.email {
        constraints.push(quote! { .format("email") });
    }
    // ... more constraints ...
    
    quote! { #(#constraints)* }
}
```

## Benefits

1. **DRY Principle**: Write validation rules once, use everywhere
2. **Type Safety**: Compile-time validation of constraint values
3. **Documentation**: API docs automatically stay in sync with validation
4. **Runtime Safety**: All endpoints validate input before processing
5. **Developer Experience**: Clear error messages for invalid requests

## Future Enhancements

Potential additions:

- [ ] Support for `multipleOf` (numeric multiples)
- [ ] Support for `enum` values (via `one_of` validator)
- [ ] Support for array item constraints
- [ ] Support for `uniqueItems` on collections
- [ ] Custom format extensions
- [ ] Conditional validation (e.g., `if`/`then`/`else`)

## See Also

- [Validatable Macro Documentation](./validatable.md)
- [OpenAPI 3.1 Specification](https://spec.openapis.org/oas/v3.1.0)
- [JSON Schema Validation](https://json-schema.org/draft/2020-12/json-schema-validation.html)

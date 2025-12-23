# Schemable Name Attribute

The `Schemable` derive macro now supports an optional `name` attribute to specify a custom schema name. By default, it uses the struct name.

## Usage

### Default name (uses struct name)

```rust
use uxar_macros::Schemable;

#[derive(Schemable)]
struct User {
    id: i32,
    name: String,
}

assert_eq!(User::NAME, "User");
```

### Custom name

```rust
use uxar_macros::Schemable;

#[derive(Schemable)]
#[schemable(name = "users")]
struct User {
    id: i32,
    name: String,
}

assert_eq!(User::NAME, "users");
```

## Benefits

- Use struct names that follow Rust conventions (PascalCase) while having database table names that follow SQL conventions (snake_case)
- Support aliasing or renaming without changing the struct name
- Access the name at compile time via the `NAME` constant
- Access the name at runtime via the `Schemable::name()` method

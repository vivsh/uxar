use uxar::db::{Bindable, Scannable};
use sqlx::postgres::PgArguments;
use sqlx::Row;

#[derive(Bindable, Scannable)]
struct SimpleUser {
    id: i32,
    name: String,
    email: String,
    active: bool,
}

#[derive(Bindable, Scannable)]
struct UserWithOptional {
    id: i32,
    name: String,
    email: Option<String>,
    phone: Option<String>,
}

#[derive(Bindable, Scannable)]
struct SkippedFields {
    id: i32,
    name: String,
    #[column(skip)]
    computed: String,
    #[field(skip)]
    internal: i32,
}

#[derive(Bindable, Scannable, Default)]
struct Address {
    street: String,
    city: String,
    country: String,
}

#[derive(Bindable, Scannable)]
struct UserWithFlattened {
    id: i32,
    name: String,
    #[field(flatten)]
    address: Address,
}

#[derive(Bindable, Scannable, serde::Serialize, serde::Deserialize)]
struct Metadata {
    key: String,
    value: String,
}

#[derive(Bindable, Scannable)]
struct UserWithJson {
    id: i32,
    name: String,
    #[field(json)]
    metadata: Metadata,
}

#[derive(Bindable, Scannable)]
struct ColumnPrecedence {
    id: i32,
    #[field(skip)]
    #[column(skip = false)]
    name: String,
}

#[derive(Bindable, Scannable, Default)]
struct NonSelectableField {
    id: i32,
    name: String,
    #[column(selectable = false)]
    password_hash: String,
}

#[test]
fn test_simple_user_bindable_compiles() {
    let user = SimpleUser {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        active: true,
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_optional_fields_bindable_compiles() {
    let user = UserWithOptional {
        id: 1,
        name: "Bob".to_string(),
        email: Some("bob@example.com".to_string()),
        phone: None,
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_skipped_fields_bindable() {
    let user = SkippedFields {
        id: 1,
        name: "Charlie".to_string(),
        computed: "ignored".to_string(),
        internal: 999,
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_flattened_bindable() {
    let user = UserWithFlattened {
        id: 1,
        name: "Dave".to_string(),
        address: Address {
            street: "123 Main St".to_string(),
            city: "Portland".to_string(),
            country: "USA".to_string(),
        },
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_json_field_bindable() {
    let user = UserWithJson {
        id: 1,
        name: "Eve".to_string(),
        metadata: Metadata {
            key: "role".to_string(),
            value: "admin".to_string(),
        },
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_column_precedence_over_field() {
    let item = ColumnPrecedence {
        id: 1,
        name: "Test".to_string(),
    };
    
    let mut args = PgArguments::default();
    let result = item.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_non_selectable_field_compiles() {
    let user = NonSelectableField {
        id: 1,
        name: "Frank".to_string(),
        password_hash: "hashed_value".to_string(),
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

#[test]
fn test_generic_type_bindable() {
    #[derive(Bindable, Scannable)]
    struct GenericUser<T: sqlx::Type<sqlx::Postgres> + for<'q> sqlx::Encode<'q, sqlx::Postgres> + for<'r> sqlx::Decode<'r, sqlx::Postgres> + Send> {
        id: i32,
        data: T,
    }
    
    let user = GenericUser {
        id: 1,
        data: "test".to_string(),
    };
    
    let mut args = PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
}

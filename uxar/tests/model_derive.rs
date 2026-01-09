use uxar::schemables::Schemable;
use uxar::validation::Validate;
use uxar::db::{Bindable, Scannable, Model};

#[derive(Model, Debug, Clone)]
#[schema(table = "users")]
struct User {
    #[column(primary_key, serial)]
    id: i32,
    
    #[validate(min_length = 3, max_length = 50)]
    name: String,
    
    #[validate(email)]
    email: String,
    
    #[column(skip)]
    password_hash: String,
    
    #[validate(min = 18, max = 120)]
    age: Option<i32>,
}

#[test]
fn test_model_implements_schemable() {
    use uxar::schemables::SchemaType;
    
    let schema_type = User::schema_type();
    
    match schema_type {
        SchemaType::Struct(schema) => {
            assert_eq!(schema.name, "User");
            assert_eq!(schema.table.name, Some(std::borrow::Cow::Borrowed("users")));
            // Verify fields exist
            assert!(schema.fields.len() >= 4);
        }
        _ => panic!("Expected SchemaType::Struct"),
    }
}

#[test]
fn test_model_implements_validate() {
    let user = User {
        id: 1,
        name: "John".to_string(),
        email: "john@example.com".to_string(),
        password_hash: "hashed".to_string(),
        age: Some(25),
    };
    
    // Should validate successfully
    assert!(user.validate().is_ok());
    
    // Test validation failure - name too short
    let invalid_user = User {
        id: 2,
        name: "Jo".to_string(), // Too short
        email: "jane@example.com".to_string(),
        password_hash: "hashed".to_string(),
        age: Some(30),
    };
    
    let result = invalid_user.validate();
    assert!(result.is_err());
}

#[test]
fn test_model_implements_bindable() {
    use sqlx::Arguments;
    
    let user = User {
        id: 1,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        password_hash: "secret".to_string(),
        age: Some(28),
    };
    
    let mut args = uxar::db::PgArguments::default();
    let result = user.bind_values(&mut args);
    assert!(result.is_ok());
    
    // Verify arguments were added (excluding skipped fields)
    // Note: password_hash is skipped, so should have 4 fields bound
    assert!(args.len() > 0);
}

#[test]
fn test_model_email_validation() {
    let invalid_email_user = User {
        id: 3,
        name: "Bob".to_string(),
        email: "not-an-email".to_string(), // Invalid email
        password_hash: "hashed".to_string(),
        age: Some(35),
    };
    
    let result = invalid_email_user.validate();
    assert!(result.is_err());
}

#[test]
fn test_model_age_validation() {
    let too_young = User {
        id: 4,
        name: "Charlie".to_string(),
        email: "charlie@example.com".to_string(),
        password_hash: "hashed".to_string(),
        age: Some(15), // Too young
    };
    
    let result = too_young.validate();
    assert!(result.is_err());
    
    let too_old = User {
        id: 5,
        name: "Diana".to_string(),
        email: "diana@example.com".to_string(),
        password_hash: "hashed".to_string(),
        age: Some(150), // Too old
    };
    
    let result = too_old.validate();
    assert!(result.is_err());
}

#[test]
fn test_model_optional_field_validation() {
    // None age should be valid
    let user_no_age = User {
        id: 6,
        name: "Eve".to_string(),
        email: "eve@example.com".to_string(),
        password_hash: "hashed".to_string(),
        age: None,
    };
    
    assert!(user_no_age.validate().is_ok());
}

#[test]
fn test_model_trait_implementation() {
    use uxar::db::Model;
    
    // Test that Model::model_schema() returns the static schema
    let schema = User::model_schema();
    assert_eq!(schema.name, "User");
    assert_eq!(schema.table.name, Some(std::borrow::Cow::Borrowed("users")));
    
    // Verify fields
    assert!(schema.fields.len() >= 4);
    
    // Test that calling it multiple times returns the same reference
    let schema2 = User::model_schema();
    assert!(std::ptr::eq(schema, schema2));
}

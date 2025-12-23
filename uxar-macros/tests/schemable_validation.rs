use uxar_macros::Schemable;
use uxar::db::{Schemable as SchemableTrait, ColumnKind};

#[derive(Schemable)]
#[schemable(crate = "uxar::db")]
struct TestStruct {
    #[column(db_column = "user_email")]
    #[validate(email)]
    email: String,

    #[validate(min_length = 3, max_length = 50)]
    username: String,

    #[validate(range = (18, 120))]
    age: i32,

    #[validate(url, non_empty)]
    website: String,

    #[validate(regex = "^[0-9]+$")]
    phone: String,

    no_validation: String,
}

#[test]
fn test_validation_in_schema() {
    let schema = TestStruct::SCHEMA;

    // Check email field
    let email_spec = &schema[0];
    assert_eq!(email_spec.name, "email");
    assert_eq!(email_spec.db_column, "user_email");
    let validation = email_spec.validation.as_ref().unwrap();
    assert!(validation.email);

    // Check username field
    let username_spec = &schema[1];
    assert_eq!(username_spec.name, "username");
    let validation = username_spec.validation.as_ref().unwrap();
    assert_eq!(validation.min_length, Some(3));
    assert_eq!(validation.max_length, Some(50));

    // Check age field with range
    let age_spec = &schema[2];
    assert_eq!(age_spec.name, "age");
    let validation = age_spec.validation.as_ref().unwrap();
    assert_eq!(validation.range, Some((18, 120)));

    // Check website field with multiple validations
    let website_spec = &schema[3];
    assert_eq!(website_spec.name, "website");
    let validation = website_spec.validation.as_ref().unwrap();
    assert!(validation.url);
    assert!(validation.non_empty);

    // Check phone field with regex
    let phone_spec = &schema[4];
    assert_eq!(phone_spec.name, "phone");
    let validation = phone_spec.validation.as_ref().unwrap();
    assert_eq!(validation.regex, Some("^[0-9]+$"));

    // Check field without validation
    let no_val_spec = &schema[5];
    assert_eq!(no_val_spec.name, "no_validation");
    assert!(no_val_spec.validation.is_none());
}

#[test]
fn test_column_and_validate_are_separate() {
    // This test just ensures the code compiles
    // The key is that #[column] attributes don't accept validation fields
    // and #[validate] attributes don't accept column fields
    #[derive(Schemable)]
    #[schemable(crate = "uxar::db")]
    struct Separated {
        #[column(db_column = "custom_col")]
        #[validate(email)]
        field: String,
    }

    let schema = Separated::SCHEMA;
    assert_eq!(schema[0].db_column, "custom_col");
    assert!(schema[0].validation.as_ref().unwrap().email);
}

#[test]
fn test_default_name() {
    #[derive(Schemable)]
    #[schemable(crate = "uxar::db")]
    struct MyStruct {
        field: String,
    }

    assert_eq!(MyStruct::NAME, "MyStruct");
}

#[test]
fn test_custom_name() {
    #[derive(Schemable)]
    #[schemable(crate = "uxar::db", name = "custom_table")]
    struct AnotherStruct {
        field: String,
    }

    assert_eq!(AnotherStruct::NAME, "custom_table");
}

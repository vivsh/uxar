use uxar::{validation::Validate, Validatable};

#[derive(Validatable)]
struct UserRegistration {
    #[validate(email, non_empty)]
    email: String,

    #[validate(min_length = 8, max_length = 100)]
    password: String,

    #[validate(min_length = 2, max_length = 50)]
    name: String,

    #[validate(min_value = "18", max_value = "120")]
    age: i32,

    #[validate(url)]
    website: Option<String>,
}

#[test]
fn test_valid_user() {
    let user = UserRegistration {
        email: "user@example.com".to_string(),
        password: "secure_password_123".to_string(),
        name: "John Doe".to_string(),
        age: 25,
        website: Some("https://example.com".to_string()),
    };

    assert!(user.validate().is_ok());
}

#[test]
fn test_invalid_email() {
    let user = UserRegistration {
        email: "not-an-email".to_string(),
        password: "secure_password_123".to_string(),
        name: "John Doe".to_string(),
        age: 25,
        website: None,
    };

    let result = user.validate();
    assert!(result.is_err());
}

#[test]
fn test_password_too_short() {
    let user = UserRegistration {
        email: "user@example.com".to_string(),
        password: "short".to_string(),
        name: "John Doe".to_string(),
        age: 25,
        website: None,
    };

    let result = user.validate();
    assert!(result.is_err());
}

#[test]
fn test_age_out_of_range() {
    let user = UserRegistration {
        email: "user@example.com".to_string(),
        password: "secure_password_123".to_string(),
        name: "John Doe".to_string(),
        age: 150,
        website: None,
    };

    let result = user.validate();
    assert!(result.is_err());
}

#[test]
fn test_invalid_url() {
    let user = UserRegistration {
        email: "user@example.com".to_string(),
        password: "secure_password_123".to_string(),
        name: "John Doe".to_string(),
        age: 25,
        website: Some("not-a-url".to_string()),
    };

    let result = user.validate();
    assert!(result.is_err());
}

#[test]
fn test_multiple_errors() {
    let user = UserRegistration {
        email: "invalid".to_string(),
        password: "123".to_string(),
        name: "".to_string(),
        age: 200,
        website: Some("bad".to_string()),
    };

    let result = user.validate();
    assert!(result.is_err());
    if let Err(report) = result {
        // Should have multiple validation errors
        assert!(report.issues.len() >= 4);
    }
}

use vyuh::Validate;

#[derive(Validate)]
struct UserRegistration {
    #[validate(email)]
    email: String,

    #[validate(min_length = 8, max_length = 100)]
    password: String,

    #[validate(min_length = 2, max_length = 50)]
    name: String,

    #[validate(min = 18, max = 120)]
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

#[derive(Validate)]
struct Nested {
    #[validate(min = 1)]
    value: i32,
}

#[derive(Validate)]
struct Container {
    #[validate(delegate)]
    nested: Nested,
    #[validate(
        min_length = 3,
        custom = "validate_custom",
        custom_schema = "custom_rule"
    )]
    custom_field: String,
}

fn validate_custom(val: &String) -> Result<(), vyuh::validation::ValidationError> {
    if val == "invalid" {
        Err(vyuh::validation::ValidationError::custom("invalid value"))
    } else {
        Ok(())
    }
}

#[derive(Validate)]
struct ParityRules {
    #[validate(min = 1, exclusive_min, max = 10, exclusive_max)]
    exclusive: i32,
    #[validate(multiple_of = 5)]
    multiple: i32,
    #[validate(min_items = 2, max_items = 3, unique_items)]
    items: Vec<i32>,
    #[validate(enum_values("draft", "published"))]
    status: String,
    #[validate(phone_e164)]
    phone: String,
    #[validate(ipv6)]
    ip6: String,
    #[validate(date)]
    date: String,
    #[validate(datetime)]
    datetime: String,
}

#[test]
fn test_runtime_schema_parity_rules_are_enforced() {
    let valid = ParityRules {
        exclusive: 5,
        multiple: 10,
        items: vec![1, 2],
        status: "draft".to_string(),
        phone: "+14155552671".to_string(),
        ip6: "2001:db8::1".to_string(),
        date: "2026-06-22".to_string(),
        datetime: "2026-06-22T10:30:00Z".to_string(),
    };
    assert!(valid.validate().is_ok());

    let invalid = ParityRules {
        exclusive: 1,
        multiple: 12,
        items: vec![1, 1],
        status: "archived".to_string(),
        phone: "4155552671".to_string(),
        ip6: "127.0.0.1".to_string(),
        date: "22-06-2026".to_string(),
        datetime: "2026-06-22 10:30:00".to_string(),
    };
    let report = invalid.validate().expect_err("invalid rules should fail");
    for field in [
        "exclusive",
        "multiple",
        "items",
        "status",
        "phone",
        "ip6",
        "date",
        "datetime",
    ] {
        assert!(report.has_error(field), "missing error for {field}");
    }
}

#[test]
fn test_delegate_and_custom() {
    let c = Container {
        nested: Nested { value: 0 },
        custom_field: "invalid".to_string(),
    };
    let result = c.validate();
    assert!(result.is_err());
    if let Err(report) = result {
        assert!(report.has_error("nested.value"));
        assert!(report.has_error("custom_field"));
    }
}

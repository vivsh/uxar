use uxar::{validation::Validate, Validatable};

#[derive(Validatable)]
struct UserProfile {
    #[validate(email, non_empty)]
    email: String,

    #[validate(min_length = 3, max_length = 50, alphanumeric)]
    username: String,

    #[validate(min_length = 8, max_length = 100)]
    password: String,

    #[validate(min_value = "18", max_value = "120")]
    age: i32,

    #[validate(url)]
    website: Option<String>,

    #[validate(uuid)]
    user_id: Option<String>,

    // Skip validation on this field
    #[validate(skip)]
    internal_notes: String,
}

#[derive(Validatable)]
struct ProductInventory {
    #[validate(non_empty, alphanumeric)]
    sku: String,

    #[validate(digits)]
    barcode: String,

    #[validate(min_value = "0")]
    quantity: i32,

    #[validate(min_value = "0.01")]
    price: f64,

    #[validate(min_length = 3, max_length = 200)]
    description: String,
}

fn main() {
    // Example 1: Valid user
    let user = UserProfile {
        email: "user@example.com".to_string(),
        username: "johndoe123".to_string(),
        password: "securePassword123".to_string(),
        age: 25,
        website: Some("https://example.com".to_string()),
        user_id: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
        internal_notes: "This field is not validated".to_string(),
    };

    match user.validate() {
        Ok(()) => println!("✓ User profile is valid"),
        Err(report) => println!("✗ Validation failed: {:?}", report),
    }

    // Example 2: Invalid user (multiple errors)
    let invalid_user = UserProfile {
        email: "not-an-email".to_string(),
        username: "ab".to_string(), // Too short
        password: "short".to_string(), // Too short
        age: 150, // Out of range
        website: Some("not-a-url".to_string()),
        user_id: Some("not-a-uuid".to_string()),
        internal_notes: "Anything goes here".to_string(),
    };

    match invalid_user.validate() {
        Ok(()) => println!("✓ User profile is valid"),
        Err(report) => {
            println!("✗ Validation failed with {} errors:", report.issues.len());
            let errors = report.into_field_map_flat();
            for (field, issues) in errors {
                for message in issues {
                    println!("  - {}: {}", field, message);
                }
            }
        }
    }

    // Example 3: Valid product
    let product = ProductInventory {
        sku: "ABC123".to_string(),
        barcode: "1234567890123".to_string(),
        quantity: 100,
        price: 29.99,
        description: "High quality product".to_string(),
    };

    match product.validate() {
        Ok(()) => println!("\n✓ Product is valid"),
        Err(report) => println!("\n✗ Validation failed: {:?}", report),
    }

    // Example 4: Invalid product
    let invalid_product = ProductInventory {
        sku: "".to_string(), // Empty
        barcode: "ABC123".to_string(), // Not digits only
        quantity: -5, // Negative
        price: 0.0, // Too low
        description: "Ok".to_string(), // Too short
    };

    match invalid_product.validate() {
        Ok(()) => println!("✓ Product is valid"),
        Err(report) => {
            println!("✗ Product validation failed with {} errors:", report.issues.len());
            let errors = report.into_field_map_flat();
            for (field, issues) in errors {
                for message in issues {
                    println!("  - {}: {}", field, message);
                }
            }
        }
    }
}

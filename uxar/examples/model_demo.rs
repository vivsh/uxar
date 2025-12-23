// Example showing how to use the Schemable derive macro

use uxar::db::{Model, Scannable, Bindable};
use uxar_macros::Schemable;

// The Schemable macro combines four derive macros:
// - SchemaInfo: Generates schema metadata for database
// - Scannable: Implements row scanning from SQL queries  
// - Bindable: Implements parameter binding for SQL queries
// - Validatable: Implements validation logic

#[derive(Schemable, Debug, Clone)]
struct User {
    /// Database column name can be different from struct field name
    #[column(db_column = "user_id")]
    id: i32,

    /// Email validation built-in
    #[column(db_column = "user_email")]
    #[validate(email)]
    email: String,

    /// Length validation
    #[validate(min_length = 3, max_length = 50)]
    username: String,

    /// Numeric value validation  
    age: i32,

    /// Optional fields work too
    #[validate(url)]
    website: Option<String>,
}

fn main() {
    // Access schema metadata - this tests that SchemaInfo is implemented
    println!("Schema name: {}", User::NAME);
    println!("Schema columns: {}", User::SCHEMA.len());
    for col in User::SCHEMA {
        println!("  - {} (db: {})", col.name, col.db_column);
        if let Some(validation) = &col.validation {
            if validation.email {
                println!("    → email validation");
            }
            if let Some(min) = validation.min_length {
                println!("    → min_length: {}", min);
            }
            if let Some(max) = validation.max_length {
                println!("    → max_length: {}", max);
            }
            if let Some((min, max)) = validation.range {
                println!("    → range: ({}, {})", min, max);
            }
        }
    }

    println!("\n✓ Schemable macro successfully implemented SchemaInfo trait");
    println!("✓ Schemable macro also implements: Scannable, Bindable, Validatable");

    // Demonstrate Scannable trait usage
    // Note: select_from() method requires Scannable trait to be in scope
    let _query = User::select_from("users");
    println!("\n✓ Scannable trait provides select_from() method");
    println!("  (Import Scannable to use: use uxar::db::Scannable;)");

    // Demonstrate Bindable trait usage  
    // Note: insert_into() method requires Bindable trait to be in scope
    let user = User {
        id: 1,
        email: "test@example.com".to_string(),
        username: "testuser".to_string(),
        age: 25,
        website: Some("https://example.com".to_string()),
    };
    let _insert_query = user.insert_into("users");
    println!("\n✓ Bindable trait provides insert_into() and update_into() methods");
    println!("  (Import Bindable to use: use uxar::db::Bindable;)");
}

// Note: Without the Schemable macro, you would need to write:
//
// #[derive(SchemaInfo, Scannable, Bindable, Validatable)]
// struct User { ... }
//
// The Schemable macro is more ergonomic and ensures all database-related
// traits are consistently implemented.

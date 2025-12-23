// Example showing db_table attribute for migration hints

use uxar::db::Model;

#[derive(Model, Debug)]
#[model(db_table = "users")]
struct User {
    #[field(primary_key, unique, db_indexed)]
    id: i32,

    #[field(unique, db_indexed)]
    email: String,

    #[field(unique_group = "username_org")]
    username: String,

    #[field(unique_group = "username_org")]
    organization_id: i32,

    #[field(db_check = "age >= 18", db_default = "18")]
    age: i32,

    // Optional field - will be nullable
    bio: Option<String>,
}

fn main() {
    println!("User schema: {} with {} columns", User::NAME, User::SCHEMA.len());
    println!("\nColumns:");
    
    for col in User::SCHEMA {
        println!("  - {} ({})", col.name, col.db_column);
        println!("    nullable: {}", col.nullable);
        
        if col.primary_key {
            println!("    PRIMARY KEY");
        }
        if col.unique {
            println!("    UNIQUE");
        }
        if let Some(ref grp) = col.unique_group {
            println!("    UNIQUE GROUP: {}", grp);
        }
        if col.db_indexed {
            println!("    INDEXED");
        }
        if let Some(ref def) = col.db_default {
            println!("    DEFAULT: {}", def);
        }
        if let Some(ref chk) = col.db_check {
            println!("    CHECK: {}", chk);
        }
    }
}
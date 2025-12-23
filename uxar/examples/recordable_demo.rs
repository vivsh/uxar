// Example showing Recordable trait generation with table_name

use uxar::db::{Schemable, Recordable};

#[derive(Schemable, Debug)]
#[schemable(table_name = "users")]
struct User {
    #[column(primary_key)]
    #[db(unique, indexed)]
    id: i32,

    #[db(unique, indexed)]
    email: String,

    #[db(unique_group = "username_org")]
    username: String,

    #[db(unique_group = "username_org")]
    organization_id: i32,

    #[db(check = "age >= 18", default = "18")]
    age: i32,

    // Optional field - will be nullable
    bio: Option<String>,
}

fn main() {
    // Get the table model for migrations
    let table_model = User::into_table_model();
    
    println!("Table: {}", table_model.name);
    println!("Columns:");
    
    for col in &table_model.columns {
        println!("  - {} ({})", col.name, col.data_type);
        println!("    nullable: {}", col.is_nullable);
        
        if col.primary_key {
            println!("    PRIMARY KEY");
        }
        if col.unique {
            println!("    UNIQUE");
        }
        if let Some(ref grp) = col.unique_group {
            println!("    UNIQUE GROUP: {}", grp);
        }
        if col.indexed {
            println!("    INDEXED");
        }
        if let Some(ref def) = col.default {
            println!("    DEFAULT: {}", def);
        }
        if let Some(ref chk) = col.check {
            println!("    CHECK: {}", chk);
        }
    }
}

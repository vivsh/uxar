// Test that schema name defaults to struct name when not specified

use uxar::db::{Schemable, SchemaInfo};

#[derive(Schemable, Debug)]
struct UserProfile {
    id: i32,
    name: String,
}

#[derive(Schemable, Debug)]
#[schemable(name = "custom_users")]
struct CustomUser {
    id: i32,
    email: String,
}

#[derive(Schemable, Debug)]
#[schemable(name = "")]  // Empty string should use struct name
struct EmptyNameTest {
    id: i32,
}

fn main() {
    // Test 1: No name specified - should use struct name
    println!("UserProfile name: {}", UserProfile::name());
    assert_eq!(UserProfile::name(), "UserProfile");
    
    // Test 2: Custom name specified
    println!("CustomUser name: {}", CustomUser::name());
    assert_eq!(CustomUser::name(), "custom_users");
    
    // Test 3: Empty string should fall back to struct name
    println!("EmptyNameTest name: {}", EmptyNameTest::name());
    assert_eq!(EmptyNameTest::name(), "EmptyNameTest");
    
    println!("\n✓ All schema name tests passed!");
}

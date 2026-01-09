use uxar::schemables::Schemable;
use uxar_macros::Schemable;

/// User account information
#[derive(Debug, Schemable)]
#[schema(table = "users", tags("user", "account"))]
struct User {
    /// Unique user identifier
    #[column(primary_key, serial)]
    id: i64,

    /// Username for login
    #[validate(min_length = 3, max_length = 50)]
    #[column(unique, index, unique_group = "user_identity")]
    username: String,

    /// User email address
    #[validate(email)]
    #[column(unique)]
    email: String,

    /// User age in years
    #[validate(min = 0, max = 150)]
    age: Option<u8>,

    /// User priority level (1-5)
    #[validate(enum_values(1, 2, 3, 4, 5))]
    priority: i32,
    
    /// Account status
    #[validate(enum_values("active", "pending", "suspended", "inactive"))]
    status: String,

    /// Account active status
    active: bool,
    
    /// User metadata stored as JSON
    #[field(json)]
    metadata: String,
    
    /// Internal flag (skipped from schema)
    #[column(skip)]
    internal: bool,
}

#[derive(Debug, Schemable)]
#[schema(tags = ["simple", "demo"])]
struct SimpleStruct {
    name: String,
    count: i32,
    
    /// Discount rate (0.0, 0.1, 0.25, 0.5)
    #[validate(enum_values(0.0, 0.1, 0.25, 0.5))]
    #[column(name = "discount_rate")]
    discount: f64,
}

fn main() {
    println!("User schema:");
    let user_schema = User::schema_type();
    println!("{:#?}\n", user_schema);

    println!("SimpleStruct schema:");
    let simple_schema = SimpleStruct::schema_type();
    println!("{:#?}", simple_schema);
}

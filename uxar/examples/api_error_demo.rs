use uxar::{ApiError, validation::{ValidationError, ValidationReport}};
use anyhow::anyhow;

// Example showing ApiError usage patterns

fn main() {
    // Example 1: Basic error creation
    let error1 = ApiError::new("Something went wrong");
    println!("Basic error: {:?}", error1);

    // Example 2: Error with specific status and code
    let error2 = ApiError::not_found("User");
    println!("Not found error: {:?}", error2);

    // Example 3: Validation error
    let error3 = ApiError::validation_error("Email is required");
    println!("Validation error: {:?}", error3);

    // Example 4: Error with custom code and details
    let error4 = ApiError::bad_request("Invalid input")
        .with_code("INVALID_FORMAT")
        .with_details("Expected JSON object");
    println!("Detailed error: {:?}", error4);

    // Example 5: Wrapping external errors
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
    let error5 = ApiError::wrap(io_error);
    println!("Wrapped error: {:?}", error5);

    // Example 6: Structured validation errors
    let mut report = ValidationReport::empty();
    report.push_root(ValidationError::new("email", "Invalid email format"));
    let error6 = ApiError::from(report);
    println!("Structured validation error: {:?}", error6);

    // Example 7: Converting from anyhow error
    let anyhow_err = anyhow!("Something failed deeply");
    let error7 = ApiError::from(anyhow_err);
    println!("Anyhow error: {:?}", error7);

    println!("\nApiError implements IntoResponse for Axum, always returning JSON responses.");
    println!("Use ? operator freely - ApiError converts from common error types automatically.");
}
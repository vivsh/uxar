/// Example demonstrating the new unified error handling in uxar
/// 
/// This shows how domain errors automatically convert to `uxar::Error`
/// through the `?` operator, providing both ergonomics and semantic categories.

use uxar::errors::{Error, ErrorKind};
use uxar::validation::{ValidationError, ValidationReport};
use uxar::db::DbError;

// Example handler showing automatic error conversion
fn example_handler() -> Result<String, Error> {
    // All these errors convert automatically via `?`
    
    // Validation error
    let mut report = ValidationReport::empty();
    report.push_root(ValidationError::custom("email is required"));
    if !report.is_empty() {
        return Err(report.into()); // Automatically becomes Error with ErrorKind::Invalid
    }
    
    // Database error - automatically categorized
    let _user = fetch_user()?; // DbError::DoesNotExist → Error with ErrorKind::NotFound
    
    Ok("Success".to_string())
}

// Example showing explicit error creation with context
fn example_with_context() -> Result<(), Error> {
    // When no domain error exists, use Error::new with context
    if some_condition() {
        return Err(Error::new(ErrorKind::NotFound)
            .with_context("User profile lookup failed"));
    }
    
    Ok(())
}

// Example showing error wrapping
fn example_wrap_external() -> Result<(), Error> {
    // Most external errors use Error::other()
    let _content = std::fs::read_to_string("/nonexistent")
        .map_err(Error::other)?;
    
    // When semantic categorization matters, use Error::wrap()
    call_external_service()
        .map_err(|e| Error::wrap(ErrorKind::Unavailable, e)
            .with_context("payment service unavailable"))?;
    
    Ok(())
}

// Mock external service
fn call_external_service() -> Result<(), std::io::Error> {
    Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "service down"))
}

// Mock functions for demonstration
fn fetch_user() -> Result<String, DbError> {
    Err(DbError::DoesNotExist)
}

fn some_condition() -> bool {
    true
}

fn main() {
    println!("=== uxar Error Handling Examples ===\n");
    
    // Example 1: Automatic error conversion
    println!("1. Automatic error conversion from DbError:");
    match example_handler() {
        Ok(_) => println!("   ✓ Handler succeeded"),
        Err(e) => {
            println!("   ✗ Display (default): {}", e);
            println!("   Compact: {}", e.display_compact());
        }
    }
    
    // Example 2: Manual error with context
    println!("\n2. Manual error creation with context:");
    match example_with_context() {
        Ok(_) => println!("   ✓ Context example succeeded"),
        Err(e) => {
            println!("   ✗ Display (default): {}", e);
            println!("\n   Verbose (for CLI):");
            print!("{}", textwrap::indent(&e.display_verbose(), "   "));
        }
    }
    
    // Example 3: Wrapping external errors
    println!("\n3. Wrapping external I/O error:");
    match example_wrap_external() {
        Ok(_) => println!("   ✓ External error example succeeded"),
        Err(e) => {
            println!("   ✗ Display (default): {}", e);
            println!("\n   Verbose (for CLI):");
            print!("{}", textwrap::indent(&e.display_verbose(), "   "));
        }
    }
    
    println!("\n=== Key Benefits ===");
    println!("• Single error type for all handlers");
    println!("• Automatic conversion via ? operator");
    println!("• Semantic error categories (NotFound, Invalid, etc.)");
    println!("• Error::other() for simple wrapping");
    println!("• Error::wrap(kind, err) for specific categorization");
    println!("• Full error chain preservation");
    println!("• Context breadcrumbs for debugging");
    println!("• display_verbose() for pretty CLI output");
    println!("• display_compact() for logging");
}

// Helper module for text indentation
mod textwrap {
    pub fn indent(text: &str, prefix: &str) -> String {
        text.lines()
            .map(|line| format!("{}{}\n", prefix, line))
            .collect()
    }
}

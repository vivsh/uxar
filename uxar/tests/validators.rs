use uxar::validation::{
    non_empty, min_len, max_len, exact_len, min_chars, max_chars,
    email, url, alphanumeric, slug, digits,
    uuid, ipv4, min, max, range, non_empty_vec, min_items, max_items, present,
};

#[test]
fn test_non_empty() {
    assert!(non_empty("hello").is_ok());
    assert!(non_empty("  spaces  ").is_ok());
    assert!(non_empty("").is_err());
    assert!(non_empty("   ").is_err());
}

#[test]
fn test_min_len() {
    let validator = min_len(5);
    assert!(validator("hello world").is_ok());
    assert!(validator("hello").is_ok());
    assert!(validator("hi").is_err());
    assert!(validator("").is_err());
}

#[test]
fn test_max_len() {
    let validator = max_len(5);
    assert!(validator("hello").is_ok());
    assert!(validator("hi").is_ok());
    assert!(validator("").is_ok());
    assert!(validator("hello world").is_err());
}

#[test]
fn test_exact_len() {
    let validator = exact_len(5);
    assert!(validator("hello").is_ok());
    assert!(validator("hi").is_err());
    assert!(validator("hello world").is_err());
    assert!(validator("").is_err());
}

#[test]
fn test_min_chars() {
    let validator = min_chars(3);
    assert!(validator("hello").is_ok());
    assert!(validator("abc").is_ok());
    assert!(validator("hi").is_err());
    assert!(validator("").is_err());
    // Unicode support
    assert!(validator("café").is_ok()); // 4 chars
    assert!(validator("ab").is_err());
}

#[test]
fn test_max_chars() {
    let validator = max_chars(5);
    assert!(validator("hello").is_ok());
    assert!(validator("hi").is_ok());
    assert!(validator("").is_ok());
    assert!(validator("hello world").is_err());
    // Unicode support
    assert!(validator("café").is_ok()); // 4 chars
}

#[test]
fn test_email() {
    // Valid emails
    assert!(email("user@example.com").is_ok());
    assert!(email("test.email@domain.co.uk").is_ok());
    assert!(email("user+tag@example.com").is_ok());
    
    // Invalid emails
    assert!(email("not-an-email").is_err());
    assert!(email("@example.com").is_err());
    assert!(email("user@").is_err());
    assert!(email("user@.com").is_err());
    assert!(email("").is_err());
    assert!(email("user@example").is_err()); // no TLD
}

#[test]
fn test_url() {
    // Valid URLs
    assert!(url("https://example.com").is_ok());
    assert!(url("http://example.com/path").is_ok());
    assert!(url("https://example.com:8080/path?query=1").is_ok());
    
    // Invalid URLs
    assert!(url("not-a-url").is_err());
    assert!(url("ftp://example.com").is_err());
    assert!(url("example.com").is_err());
    assert!(url("://example.com").is_err());
    assert!(url("").is_err());
}

#[test]
fn test_alphanumeric() {
    assert!(alphanumeric("abc123").is_ok());
    assert!(alphanumeric("ABC").is_ok());
    assert!(alphanumeric("123").is_ok());
    assert!(alphanumeric("a").is_ok());
    assert!(alphanumeric("").is_ok());
    
    assert!(alphanumeric("abc-123").is_err());
    assert!(alphanumeric("hello_world").is_err());
    assert!(alphanumeric("test@example").is_err());
    assert!(alphanumeric("hello world").is_err());
}

#[test]
fn test_slug() {
    assert!(slug("hello-world").is_ok());
    assert!(slug("my_slug").is_ok());
    assert!(slug("slug123").is_ok());
    assert!(slug("a").is_ok());
    assert!(slug("").is_ok());
    
    assert!(slug("hello@world").is_err());
    assert!(slug("hello world").is_err());
    assert!(slug("hello.world").is_err());
}

#[test]
fn test_digits() {
    assert!(digits("123456").is_ok());
    assert!(digits("0").is_ok());
    assert!(digits("999").is_ok());
    assert!(digits("").is_ok());
    
    assert!(digits("123abc").is_err());
    assert!(digits("12.34").is_err());
    assert!(digits("one two three").is_err());
    assert!(digits("123-456").is_err());
}

#[test]
fn test_uuid() {
    // Valid UUIDs
    assert!(uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
    assert!(uuid("6ba7b810-9dad-11d1-80b4-00c04fd430c8").is_ok());
    assert!(uuid("ABCDEF01-2345-6789-ABCD-EF0123456789").is_ok()); // uppercase
    
    // Invalid UUIDs
    assert!(uuid("550e8400e29b41d4a716446655440000").is_err()); // missing dashes
    assert!(uuid("550e8400-e29b-41d4-a716-44665544000").is_err()); // too short
    assert!(uuid("550e8400-e29b-41d4-a716-4466554400000").is_err()); // too long
    assert!(uuid("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx").is_err());
    assert!(uuid("").is_err());
}

#[test]
fn test_ipv4() {
    // Valid IPv4
    assert!(ipv4("192.168.1.1").is_ok());
    assert!(ipv4("0.0.0.0").is_ok());
    assert!(ipv4("255.255.255.255").is_ok());
    assert!(ipv4("127.0.0.1").is_ok());
    
    // Invalid IPv4
    assert!(ipv4("256.1.1.1").is_err()); // out of range
    assert!(ipv4("192.168.1").is_err()); // missing octet
    assert!(ipv4("192.168.1.1.1").is_err()); // too many octets
    assert!(ipv4("192.168.a.1").is_err()); // non-numeric
    assert!(ipv4("192.168..1").is_err());
    assert!(ipv4("").is_err());
}

#[test]
fn test_min_numeric() {
    let validator = min(10);
    assert!(validator(&15).is_ok());
    assert!(validator(&10).is_ok());
    assert!(validator(&9).is_err());
    assert!(validator(&-100).is_err());
}

#[test]
fn test_max_numeric() {
    let validator = max(100);
    assert!(validator(&50).is_ok());
    assert!(validator(&100).is_ok());
    assert!(validator(&101).is_err());
    assert!(validator(&200).is_err());
}

#[test]
fn test_range_numeric() {
    let validator = range(10, 100);
    assert!(validator(&50).is_ok());
    assert!(validator(&10).is_ok());
    assert!(validator(&100).is_ok());
    assert!(validator(&9).is_err());
    assert!(validator(&101).is_err());
    assert!(validator(&-50).is_err());
}

#[test]
fn test_min_items() {
    let validator = min_items::<i32>(2);
    assert!(validator(&vec![1, 2, 3]).is_ok());
    assert!(validator(&vec![1, 2]).is_ok());
    assert!(validator(&vec![1]).is_err());
    assert!(validator(&vec![]).is_err());
}

#[test]
fn test_max_items() {
    let validator = max_items::<i32>(2);
    assert!(validator(&vec![1, 2]).is_ok());
    assert!(validator(&vec![1]).is_ok());
    assert!(validator(&vec![]).is_ok());
    assert!(validator(&vec![1, 2, 3]).is_err());
}

#[test]
fn test_non_empty_vec() {
    assert!(non_empty_vec(&vec![1, 2, 3]).is_ok());
    assert!(non_empty_vec(&vec![1]).is_ok());
    assert!(non_empty_vec::<i32>(&vec![]).is_err());
}

#[test]
fn test_present_option() {
    assert!(present(&Some(42)).is_ok());
    assert!(present(&Some("hello")).is_ok());
    assert!(present::<i32>(&None).is_err());
}

#[test]
fn test_error_messages() {
    // Verify error messages are set correctly
    let err = non_empty("").unwrap_err();
    assert_eq!(err.code, "blank");
    assert_eq!(err.message, "This field may not be blank.");
    
    let err = email("invalid").unwrap_err();
    assert_eq!(err.code, "email");
    assert_eq!(err.message, "Enter a valid email address.");
    
    let err = (min(10))(&5).unwrap_err();
    assert_eq!(err.code, "min_value");
    assert!(err.message.contains("10"));
}

#[test]
fn test_validator_composition() {
    // Test that multiple validators can be chained
    let non_empty_validator = non_empty;
    let min_validator = min_len(5);
    let max_validator = max_len(20);
    
    let test_str = "hello world";
    assert!(non_empty_validator(test_str).is_ok());
    assert!(min_validator(test_str).is_ok());
    assert!(max_validator(test_str).is_ok());
    
    let test_str = "hi";
    assert!(non_empty_validator(test_str).is_ok());
    assert!(min_validator(test_str).is_err());
    assert!(max_validator(test_str).is_ok());
}

#[test]
fn test_numeric_range_types() {
    // Test range validator with different numeric types
    let range_i32 = range(0i32, 100i32);
    assert!(range_i32(&50).is_ok());
    assert!(range_i32(&0).is_ok());
    assert!(range_i32(&100).is_ok());
    assert!(range_i32(&101).is_err());
    assert!(range_i32(&-1).is_err());
    
    let range_f64 = range(0.0f64, 1.0f64);
    assert!(range_f64(&0.5).is_ok());
    assert!(range_f64(&0.0).is_ok());
    assert!(range_f64(&1.0).is_ok());
    assert!(range_f64(&1.1).is_err());
    assert!(range_f64(&-0.1).is_err());
}

#[test]
fn test_unicode_validators() {
    // Test that Unicode is handled correctly
    
    // Email with unicode
    assert!(email("用户@example.com").is_err()); // non-ASCII in local part
    
    // URL with unicode path
    assert!(url("https://example.com/café").is_ok());
    
    // Character count with Unicode
    let validator = min_chars(2);
    assert!(validator("👍🏻").is_ok()); // 2 grapheme clusters
    
    let validator = max_chars(5);
    assert!(validator("café").is_ok()); // 4 chars
    assert!(validator("hello").is_ok()); // 5 chars
}

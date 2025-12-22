use std::borrow::Cow;
use regex::Regex;

use super::validation::{ValidationError};

/// ---------- shared helpers ----------

#[inline]
fn err(code: &'static str, msg: impl Into<Cow<'static, str>>) -> ValidationError {
    ValidationError::new(code, msg)
}

/// ---------- presence / option ----------

/// Checks that an Option contains Some value.
/// For required fields, prefer making the field non-optional in your struct.
pub fn present<T>(v: &Option<T>) -> Result<(), ValidationError> {
    if v.is_some() {
        Ok(())
    } else {
        Err(err("required", "This field is required."))
    }
}

/// ---------- string validators ----------

/// Checks that a string is not empty or whitespace-only.
pub fn non_empty(s: &str) -> Result<(), ValidationError> {
    if s.trim().is_empty() {
        Err(err("blank", "This field may not be blank."))
    } else {
        Ok(())
    }
}

/// Validates minimum byte length (not character count).
/// For Unicode-aware validation, use `min_chars`.
pub fn min_len(n: usize) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        if s.len() < n {
            Err(err(
                "min_length",
                format!("Ensure this field has at least {n} characters."),
            ))
        } else {
            Ok(())
        }
    }
}

/// Validates maximum byte length (not character count).
/// For Unicode-aware validation, use `max_chars`.
pub fn max_len(n: usize) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        if s.len() > n {
            Err(err(
                "max_length",
                format!("Ensure this field has at most {n} characters."),
            ))
        } else {
            Ok(())
        }
    }
}

pub fn exact_len(n: usize) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        if s.len() != n {
            Err(err(
                "exact_length",
                format!("Ensure this field has exactly {n} characters."),
            ))
        } else {
            Ok(())
        }
    }
}

/// Validates minimum character count (Unicode-aware).
pub fn min_chars(n: usize) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        let count = s.chars().count();
        if count < n {
            Err(err(
                "min_chars",
                format!("Ensure this field has at least {n} characters."),
            ))
        } else {
            Ok(())
        }
    }
}

/// Validates maximum character count (Unicode-aware).
pub fn max_chars(n: usize) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        let count = s.chars().count();
        if count > n {
            Err(err(
                "max_chars",
                format!("Ensure this field has at most {n} characters."),
            ))
        } else {
            Ok(())
        }
    }
}

/// RFC-ish, pragmatic email (copied philosophy from validator crate)
pub fn email(s: &str) -> Result<(), ValidationError> {
    static EMAIL_RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9-]+(\.[a-z0-9-]+)+$")
            .expect("valid email regex")
    });

    if EMAIL_RE.is_match(s) {
        Ok(())
    } else {
        Err(err("email", "Enter a valid email address."))
    }
}

/// Validates against a regex pattern. Accepts static or owned Regex.
pub fn regex(re: &'static Regex) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        if re.is_match(s) {
            Ok(())
        } else {
            Err(err("invalid", "Invalid format."))
        }
    }
}

/// Validates against a regex pattern with custom error message.
pub fn regex_with_msg(
    re: &'static Regex,
    msg: &'static str,
) -> impl Fn(&str) -> Result<(), ValidationError> {
    move |s| {
        if re.is_match(s) {
            Ok(())
        } else {
            Err(err("invalid", msg))
        }
    }
}

/// Validates URL format (http/https).
pub fn url(s: &str) -> Result<(), ValidationError> {
    static URL_RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"^https?://[^\s/$.?#].[^\s]*$")
            .expect("valid url regex")
    });

    if URL_RE.is_match(s) {
        Ok(())
    } else {
        Err(err("url", "Enter a valid URL."))
    }
}

/// Validates alphanumeric characters only (a-z, A-Z, 0-9).
pub fn alphanumeric(s: &str) -> Result<(), ValidationError> {
    if s.chars().all(|c| c.is_alphanumeric()) {
        Ok(())
    } else {
        Err(err("alphanumeric", "Only letters and numbers are allowed."))
    }
}

/// Validates alphanumeric with dashes and underscores (slug-like).
pub fn slug(s: &str) -> Result<(), ValidationError> {
    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        Ok(())
    } else {
        Err(err("slug", "Only letters, numbers, dashes, and underscores are allowed."))
    }
}

/// Validates digits only (0-9).
pub fn digits(s: &str) -> Result<(), ValidationError> {
    if s.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err(err("digits", "Only digits are allowed."))
    }
}

/// Validates UUID format (8-4-4-4-12 hex pattern).
pub fn uuid(s: &str) -> Result<(), ValidationError> {
    static UUID_RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
            .expect("valid uuid regex")
    });

    if UUID_RE.is_match(s) {
        Ok(())
    } else {
        Err(err("uuid", "Enter a valid UUID."))
    }
}

/// Validates IP address (v4).
pub fn ipv4(s: &str) -> Result<(), ValidationError> {
    static IPV4_RE: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
        Regex::new(r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$")
            .expect("valid ipv4 regex")
    });

    if IPV4_RE.is_match(s) {
        Ok(())
    } else {
        Err(err("ipv4", "Enter a valid IPv4 address."))
    }
}

/// ---------- numeric validators ----------

pub fn min<T>(min: T) -> impl Fn(&T) -> Result<(), ValidationError>
where
    T: PartialOrd + std::fmt::Display,
{
    move |v| {
        if *v < min {
            Err(err(
                "min_value",
                format!("Ensure this value is greater than or equal to {min}."),
            ))
        } else {
            Ok(())
        }
    }
}

pub fn max<T>(max: T) -> impl Fn(&T) -> Result<(), ValidationError>
where
    T: PartialOrd + std::fmt::Display,
{
    move |v| {
        if *v > max {
            Err(err(
                "max_value",
                format!("Ensure this value is less than or equal to {max}."),
            ))
        } else {
            Ok(())
        }
    }
}

pub fn range<T>(min: T, max: T) -> impl Fn(&T) -> Result<(), ValidationError>
where
    T: PartialOrd + std::fmt::Display,
{
    move |v| {
        if *v < min || *v > max {
            Err(err(
                "value_range",
                format!("Ensure this value is between {min} and {max}."),
            ))
        } else {
            Ok(())
        }
    }
}

/// ---------- collections ----------

/// Checks that a collection is not empty.
pub fn non_empty_vec<T>(v: &[T]) -> Result<(), ValidationError> {
    if v.is_empty() {
        Err(err("empty", "This list must not be empty."))
    } else {
        Ok(())
    }
}


pub fn min_items<T>(n: usize) -> impl for<'a> Fn(&'a [T]) -> Result<(), ValidationError> {
    move |v: &[T]| {
        if v.len() < n {
            Err(err("min_items", format!("Ensure this list has at least {n} items.")))
        } else {
            Ok(())
        }
    }
}

pub fn max_items<T>(n: usize) -> impl for<'a> Fn(&'a [T]) -> Result<(), ValidationError> {
    move |v: &[T]| {
        if v.len() > n {
            Err(err("max_items", format!("Ensure this list has at most {n} items.")))
        } else {
            Ok(())
        }
    }
}

/// ---------- boolean ----------

pub fn must_be_true(v: &bool) -> Result<(), ValidationError> {
    if *v {
        Ok(())
    } else {
        Err(err("required", "This field must be true."))
    }
}

/// ---------- enums / choices ----------

/// Validates that value is one of the allowed choices.
pub fn one_of<T: PartialEq + 'static>(
    allowed: &'static [T],
) -> impl Fn(&T) -> Result<(), ValidationError> {
    move |v| {
        if allowed.contains(v) {
            Ok(())
        } else {
            Err(err(
                "invalid_choice",
                "Selected value is not a valid choice.",
            ))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use regex::Regex;

    // presence
    #[test]
    fn test_present_some() {
        assert!(present(&Some(1)).is_ok());
    }
    #[test]
    fn test_present_none() {
        assert!(present(&None::<i32>).is_err());
    }

    // strings
    #[test]
    fn test_non_empty() {
        assert!(non_empty("a").is_ok());
        assert!(non_empty("").is_err());
        assert!(non_empty("   ").is_err());
    }

    #[test]
    fn test_min_max_exact_len() {
        let min3 = min_len(3);
        assert!(min3("ab").is_err());
        assert!(min3("abc").is_ok());

        let max5 = max_len(5);
        assert!(max5("abcdef").is_err());
        assert!(max5("abc").is_ok());

        let ex4 = exact_len(4);
        assert!(ex4("abcd").is_ok());
        assert!(ex4("abc").is_err());
    }

    #[test]
    fn test_min_max_chars() {
        let min3 = min_chars(3);
        assert!(min3("世界").is_err()); // 2 chars
        assert!(min3("世界!").is_ok()); // 3 chars

        let max5 = max_chars(5);
        assert!(max5("Hello, 世界").is_err()); // 9 chars
        assert!(max5("世界").is_ok()); // 2 chars
    }

    #[test]
    fn test_email_and_regex() {
        assert!(email("user@example.com").is_ok());
        assert!(email("not-an-email").is_err());

        static DIGITS: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+$").expect("regex"));
        let digit_validator = regex(&*DIGITS);
        assert!(digit_validator("12345").is_ok());
        assert!(digit_validator("12a45").is_err());
    }

    #[test]
    fn test_new_string_validators() {
        assert!(url("https://example.com").is_ok());
        assert!(url("not-a-url").is_err());

        assert!(alphanumeric("abc123").is_ok());
        assert!(alphanumeric("abc-123").is_err());

        assert!(slug("hello-world_123").is_ok());
        assert!(slug("hello world").is_err());

        assert!(digits("12345").is_ok());
        assert!(digits("123a5").is_err());

        assert!(uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(uuid("not-a-uuid").is_err());

        assert!(ipv4("192.168.1.1").is_ok());
        assert!(ipv4("999.999.999.999").is_err());
    }

    // numeric
    #[test]
    fn test_min_max_range() {
        let min5 = min(5);
        assert!(min5(&3).is_err());
        assert!(min5(&5).is_ok());

        let max10 = max(10);
        assert!(max10(&11).is_err());
        assert!(max10(&10).is_ok());

        let rng = range(1, 3);
        assert!(rng(&0).is_err());
        assert!(rng(&2).is_ok());
        assert!(rng(&4).is_err());
    }

    // collections
    #[test]
    fn test_non_empty_vec_and_min_max_items() {
        assert!(non_empty_vec(&vec![1]).is_ok());
        assert!(non_empty_vec(&Vec::<i32>::new()).is_err());

        let min2 = min_items(2);
        assert!(min2(&vec![1]).is_err());
        assert!(min2(&vec![1, 2]).is_ok());

        let max3 = max_items(3);
        assert!(max3(&vec![1, 2, 3, 4]).is_err());
        assert!(max3(&vec![1, 2]).is_ok());
    }

    // boolean
    #[test]
    fn test_must_be_true() {
        assert!(must_be_true(&true).is_ok());
        assert!(must_be_true(&false).is_err());
    }

    // choices
    #[test]
    fn test_one_of() {
        static CHOICES: &[i32] = &[1, 2, 3];
        let one = one_of(CHOICES);
        assert!(one(&2).is_ok());
        assert!(one(&4).is_err());
    }
}
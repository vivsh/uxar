use uxar::auth::{check_password, make_password, unusable_password};

#[test]
fn round_trip_default() {
    let pw = "s3cr3tP@ss";
    let encoded = make_password(pw, None, None).expect("make_password");
    let ok = check_password(pw, &encoded).expect("check_password");
    assert!(ok, "password should validate");
}

#[test]
fn same_salt_repeatable() {
    let pw = "hello123";
    let salt = "fixed-salt-123";
    let a = make_password(pw, Some(salt), Some("pbkdf2_sha256")).unwrap();
    let b = make_password(pw, Some(salt), Some("pbkdf2_sha256")).unwrap();
    assert_eq!(a, b);
    assert!(check_password(pw, &a).unwrap());
}

#[test]
fn wrong_password_fails() {
    let pw = "one";
    let other = "two";
    let encoded = make_password(pw, None, None).unwrap();
    assert!(!check_password(other, &encoded).unwrap());
}

#[test]
fn unusable_password_fails_check() {
    let unusable = unusable_password().unwrap();
    assert!(unusable.starts_with("!"));
    assert!(!check_password("anything", &unusable).unwrap());
}

use super::*;
#[test]
fn keys_are_explicit_and_unicode_is_preserved() {
    assert_eq!(webdriver_key("Enter").unwrap(), "\u{e007}");
    assert_eq!(webdriver_key("中").unwrap(), "中");
    assert!(webdriver_key("F13").is_err());
    assert!(webdriver_key("").is_err());
}

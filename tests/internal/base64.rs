// Mirrors TS SDK tests/base64.test.ts.

use anthropic_sdk::internal::base64::{
    from_base64, to_base64_bytes, to_base64_optional, to_base64_str,
};

#[test]
fn to_base64_matches_ts_utility_cases() {
    assert_eq!(to_base64_str("hello world"), "aGVsbG8gd29ybGQ=");
    assert_eq!(
        to_base64_bytes([104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]),
        "aGVsbG8gd29ybGQ="
    );
    assert_eq!(to_base64_optional(None), "");
    assert_eq!(
        to_base64_bytes([
            229, 102, 215, 230, 65, 22, 46, 87, 243, 176, 99, 99, 31, 174, 8, 242, 83, 142, 169,
            64, 122, 123, 193, 71,
        ]),
        "5WbX5kEWLlfzsGNjH64I8lOOqUB6e8FH"
    );
    assert_eq!(to_base64_str("✓"), "4pyT");
    assert_eq!(to_base64_bytes([226, 156, 147]), "4pyT");
}

#[test]
fn from_base64_matches_ts_utility_cases() {
    assert_eq!(
        from_base64("aGVsbG8gd29ybGQ=").unwrap(),
        vec![104, 101, 108, 108, 111, 32, 119, 111, 114, 108, 100]
    );
    assert_eq!(from_base64("").unwrap(), Vec::<u8>::new());
    assert_eq!(
        from_base64("5WbX5kEWLlfzsGNjH64I8lOOqUB6e8FH").unwrap(),
        vec![
            229, 102, 215, 230, 65, 22, 46, 87, 243, 176, 99, 99, 31, 174, 8, 242, 83, 142, 169,
            64, 122, 123, 193, 71,
        ]
    );
    assert_eq!(from_base64("4pyT").unwrap(), vec![226, 156, 147]);
}

#[test]
fn from_base64_reports_invalid_input() {
    assert!(
        from_base64("a")
            .unwrap_err()
            .to_string()
            .contains("invalid base64")
    );
    assert!(
        from_base64("!!!!")
            .unwrap_err()
            .to_string()
            .contains("invalid base64")
    );
    assert!(
        from_base64("abc=d")
            .unwrap_err()
            .to_string()
            .contains("invalid base64")
    );
}

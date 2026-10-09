// Maps to: TS internal/utils/values.ts
//
//! Small value utilities mirroring the TypeScript SDK's internal helpers.
//!
//! JavaScript's `unknown` values do not translate directly to Rust, so helpers
//! that inspect arbitrary values operate on `serde_json::Value` or typed Rust
//! inputs while preserving the TS semantics that matter to request building,
//! validation, and tests.

use crate::core::error::ApiError;
use serde_json::{Map, Value};

/// Maps to TS `isAbsoluteURL(url)`.
pub fn is_absolute_url(url: &str) -> bool {
    let mut chars = url.bytes();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }

    for byte in chars {
        if byte == b':' {
            return true;
        }
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-')) {
            return false;
        }
    }
    false
}

/// TS-style alias for [`is_absolute_url`].
#[allow(non_snake_case)]
pub fn isAbsoluteURL(url: &str) -> bool {
    is_absolute_url(url)
}

/// Maps to TS `isArray` for JSON-like values.
pub fn is_array(value: &Value) -> bool {
    value.is_array()
}

/// TS-style alias for [`is_array`].
#[allow(non_snake_case)]
pub fn isArray(value: &Value) -> bool {
    is_array(value)
}

/// Rust equivalent of TS `isReadonlyArray` for JSON-like values.
pub fn is_readonly_array(value: &Value) -> bool {
    is_array(value)
}

/// TS-style alias for [`is_readonly_array`].
#[allow(non_snake_case)]
pub fn isReadonlyArray(value: &Value) -> bool {
    is_readonly_array(value)
}

/// Maps to TS `maybeObj(x)`: returns the object as-is or an empty object for
/// null/non-object values.
pub fn maybe_obj(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

/// TS-style alias for [`maybe_obj`].
#[allow(non_snake_case)]
pub fn maybeObj(value: Option<&Value>) -> Map<String, Value> {
    maybe_obj(value)
}

/// Maps to TS `isEmptyObj` for JSON-like values.
pub fn is_empty_obj(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Object(map)) => map.is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        Some(_) => true,
    }
}

/// TS-style alias for [`is_empty_obj`].
#[allow(non_snake_case)]
pub fn isEmptyObj(value: Option<&Value>) -> bool {
    is_empty_obj(value)
}

/// Maps to TS `hasOwn(obj, key)`.
pub fn has_own(map: &Map<String, Value>, key: &str) -> bool {
    map.contains_key(key)
}

/// TS-style alias for [`has_own`].
#[allow(non_snake_case)]
pub fn hasOwn(map: &Map<String, Value>, key: &str) -> bool {
    has_own(map, key)
}

/// Maps to TS `isObj(obj)` for JSON-like values: object, not null, not array.
pub fn is_obj(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

/// TS-style alias for [`is_obj`].
#[allow(non_snake_case)]
pub fn isObj(value: &Value) -> bool {
    is_obj(value)
}

/// Maps to TS `ensurePresent(value)` for Rust optional values.
pub fn ensure_present<T>(value: Option<T>) -> Result<T, ApiError> {
    value.ok_or_else(|| {
        ApiError::Sdk("Expected a value to be given but received undefined instead.".to_owned())
    })
}

/// TS-style alias for [`ensure_present`].
#[allow(non_snake_case)]
pub fn ensurePresent<T>(value: Option<T>) -> Result<T, ApiError> {
    ensure_present(value)
}

/// Maps to TS `validatePositiveInteger(name, n)`.
///
/// The TS helper accepts zero despite its error wording; this function preserves
/// that behavior by rejecting only negative integer values.
pub fn validate_positive_integer(name: &str, value: &Value) -> Result<f64, ApiError> {
    let Some(number) = value.as_f64() else {
        return Err(ApiError::Sdk(format!("{name} must be an integer")));
    };
    if !number.is_finite() || number.fract() != 0.0 {
        return Err(ApiError::Sdk(format!("{name} must be an integer")));
    }
    if number < 0.0 {
        return Err(ApiError::Sdk(format!("{name} must be a positive integer")));
    }
    Ok(number)
}

/// TS-style alias for [`validate_positive_integer`].
#[allow(non_snake_case)]
pub fn validatePositiveInteger(name: &str, value: &Value) -> Result<f64, ApiError> {
    validate_positive_integer(name, value)
}

/// Maps to TS `coerceInteger(value)`.
pub fn coerce_integer(value: &Value) -> Result<f64, ApiError> {
    match value {
        Value::Number(number) => number
            .as_f64()
            .map(js_math_round)
            .ok_or_else(|| coerce_number_error(value)),
        Value::String(text) => Ok(parse_int_radix_10(text)),
        _ => Err(coerce_number_error(value)),
    }
}

/// TS-style alias for [`coerce_integer`].
#[allow(non_snake_case)]
pub fn coerceInteger(value: &Value) -> Result<f64, ApiError> {
    coerce_integer(value)
}

/// Maps to TS `coerceFloat(value)`.
pub fn coerce_float(value: &Value) -> Result<f64, ApiError> {
    match value {
        Value::Number(number) => number.as_f64().ok_or_else(|| coerce_number_error(value)),
        Value::String(text) => Ok(parse_float_like_js(text)),
        _ => Err(coerce_number_error(value)),
    }
}

/// TS-style alias for [`coerce_float`].
#[allow(non_snake_case)]
pub fn coerceFloat(value: &Value) -> Result<f64, ApiError> {
    coerce_float(value)
}

/// Maps to TS `coerceBoolean(value)`.
pub fn coerce_boolean(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::String(value) => value == "true",
        Value::Null => false,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// TS-style alias for [`coerce_boolean`].
#[allow(non_snake_case)]
pub fn coerceBoolean(value: &Value) -> bool {
    coerce_boolean(value)
}

/// Maps to TS `maybeCoerceInteger(value)`.
pub fn maybe_coerce_integer(value: Option<&Value>) -> Result<Option<f64>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => coerce_integer(value).map(Some),
    }
}

/// TS-style alias for [`maybe_coerce_integer`].
#[allow(non_snake_case)]
pub fn maybeCoerceInteger(value: Option<&Value>) -> Result<Option<f64>, ApiError> {
    maybe_coerce_integer(value)
}

/// Maps to TS `maybeCoerceFloat(value)`.
pub fn maybe_coerce_float(value: Option<&Value>) -> Result<Option<f64>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => coerce_float(value).map(Some),
    }
}

/// TS-style alias for [`maybe_coerce_float`].
#[allow(non_snake_case)]
pub fn maybeCoerceFloat(value: Option<&Value>) -> Result<Option<f64>, ApiError> {
    maybe_coerce_float(value)
}

/// Maps to TS `maybeCoerceBoolean(value)`.
pub fn maybe_coerce_boolean(value: Option<&Value>) -> Option<bool> {
    match value {
        None | Some(Value::Null) => None,
        Some(value) => Some(coerce_boolean(value)),
    }
}

/// TS-style alias for [`maybe_coerce_boolean`].
#[allow(non_snake_case)]
pub fn maybeCoerceBoolean(value: Option<&Value>) -> Option<bool> {
    maybe_coerce_boolean(value)
}

/// Maps to TS `safeJSON(text)`.
pub fn safe_json(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// TS-style alias for [`safe_json`].
#[allow(non_snake_case)]
pub fn safeJSON(text: &str) -> Option<Value> {
    safe_json(text)
}

/// Maps to TS `pop(obj, key)` for JSON maps.
pub fn pop(map: &mut Map<String, Value>, key: &str) -> Option<Value> {
    map.remove(key)
}

fn js_math_round(value: f64) -> f64 {
    // JavaScript Math.round(x) is floor(x + 0.5), which differs from Rust's
    // half-away-from-zero rounding for negative half values.
    (value + 0.5).floor()
}

fn parse_int_radix_10(text: &str) -> f64 {
    let trimmed = text.trim_start();
    let mut chars = trimmed.char_indices().peekable();
    let mut sign = 1.0;
    let mut digits_start = 0;

    if let Some((_, ch)) = chars.peek().copied() {
        if ch == '-' || ch == '+' {
            sign = if ch == '-' { -1.0 } else { 1.0 };
            chars.next();
            digits_start = ch.len_utf8();
        }
    }

    let mut digits_end = digits_start;
    let mut saw_digit = false;
    for (index, ch) in chars {
        if ch.is_ascii_digit() {
            saw_digit = true;
            digits_end = index + ch.len_utf8();
        } else {
            break;
        }
    }

    if !saw_digit {
        return f64::NAN;
    }

    trimmed[digits_start..digits_end]
        .parse::<f64>()
        .map(|value| sign * value)
        .unwrap_or(f64::NAN)
}

fn parse_float_like_js(text: &str) -> f64 {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return f64::NAN;
    }

    let mut index = 0;
    let bytes = trimmed.as_bytes();
    if matches!(bytes.get(index), Some(b'+' | b'-')) {
        index += 1;
    }

    if trimmed[index..].starts_with("Infinity") {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    let mut digits_before = 0;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
        digits_before += 1;
    }

    let mut digits_after = 0;
    if matches!(bytes.get(index), Some(b'.')) {
        index += 1;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
            digits_after += 1;
        }
    }

    if digits_before + digits_after == 0 {
        return f64::NAN;
    }

    let exponent_start = index;
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_digits_start = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if exponent_digits_start == index {
            index = exponent_start;
        }
    }

    trimmed[..index].parse::<f64>().unwrap_or(f64::NAN)
}

fn coerce_number_error(value: &Value) -> ApiError {
    ApiError::Sdk(format!(
        "Could not coerce {} (type: {}) into a number",
        js_display(value),
        js_typeof(value)
    ))
}

fn js_typeof(value: &Value) -> &'static str {
    match value {
        Value::Null => "object",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "object",
    }
}

fn js_display(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(items) => items.iter().map(js_display).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absolute_url_detection_matches_ts_scheme_regexp() {
        assert!(is_absolute_url("https://example.com"));
        assert!(is_absolute_url("HTTP://example.com"));
        assert!(is_absolute_url("a+b.c-:value"));
        assert!(!is_absolute_url("/v1/messages"));
        assert!(!is_absolute_url("1http://example.com"));
        assert!(!is_absolute_url("http//example.com"));
        assert!(!is_absolute_url("http_/example.com:"));
    }

    #[test]
    fn object_helpers_match_json_like_ts_semantics() {
        let value = json!({"foo": 1});
        let array = json!([1, 2]);
        let null = Value::Null;
        assert!(is_obj(&value));
        assert!(!is_obj(&array));
        assert!(is_array(&array));
        assert!(is_empty_obj(None));
        assert!(is_empty_obj(Some(&null)));
        assert!(!is_empty_obj(Some(&value)));
        assert_eq!(maybe_obj(Some(&json!("x"))).len(), 0);
        assert!(has_own(value.as_object().unwrap(), "foo"));
        assert_eq!(pop(&mut maybe_obj(Some(&value)), "foo"), Some(json!(1)));
    }

    #[test]
    fn integer_validation_and_coercion_match_ts_helpers() {
        assert_eq!(validate_positive_integer("n", &json!(0)).unwrap(), 0.0);
        assert_eq!(validate_positive_integer("n", &json!(42)).unwrap(), 42.0);
        assert!(
            validate_positive_integer("n", &json!(1.2))
                .unwrap_err()
                .to_string()
                .contains("n must be an integer")
        );
        assert!(
            validate_positive_integer("n", &json!(-1))
                .unwrap_err()
                .to_string()
                .contains("n must be a positive integer")
        );

        assert_eq!(coerce_integer(&json!(1.49)).unwrap(), 1.0);
        assert_eq!(coerce_integer(&json!(1.5)).unwrap(), 2.0);
        assert_eq!(coerce_integer(&json!(-1.5)).unwrap(), -1.0);
        assert_eq!(coerce_integer(&json!("42px")).unwrap(), 42.0);
        assert_eq!(coerce_integer(&json!("0x10")).unwrap(), 0.0);
        assert!(coerce_integer(&json!("x")).unwrap().is_nan());
        assert!(
            coerce_integer(&json!({"x": 1}))
                .unwrap_err()
                .to_string()
                .contains("Could not coerce [object Object] (type: object) into a number")
        );
    }

    #[test]
    fn float_boolean_maybe_and_safe_json_helpers_match_ts() {
        assert_eq!(coerce_float(&json!("1.25px")).unwrap(), 1.25);
        assert_eq!(coerce_float(&json!("1e2")).unwrap(), 100.0);
        assert_eq!(coerce_float(&json!("1e+")).unwrap(), 1.0);
        assert!(coerce_float(&json!("nope")).unwrap().is_nan());
        assert!(
            coerce_float(&json!([1]))
                .unwrap_err()
                .to_string()
                .contains("Could not coerce 1 (type: object) into a number")
        );

        assert!(coerce_boolean(&json!(true)));
        assert!(coerce_boolean(&json!("true")));
        assert!(!coerce_boolean(&json!("false")));
        assert!(!coerce_boolean(&json!(0)));
        assert!(coerce_boolean(&json!({})));

        assert_eq!(maybe_coerce_integer(None).unwrap(), None);
        assert_eq!(maybe_coerce_integer(Some(&Value::Null)).unwrap(), None);
        assert_eq!(maybe_coerce_float(Some(&json!("2.5"))).unwrap(), Some(2.5));
        assert_eq!(maybe_coerce_boolean(Some(&json!("true"))), Some(true));
        assert_eq!(maybe_coerce_boolean(Some(&Value::Null)), None);

        assert_eq!(safe_json("{\"ok\":true}"), Some(json!({"ok": true})));
        assert_eq!(safe_json("not json"), None);
    }

    #[test]
    fn ts_style_aliases_are_available() {
        assert!(isAbsoluteURL("https://example.com"));
        assert!(isArray(&json!([])));
        assert!(isReadonlyArray(&json!([])));
        assert!(isObj(&json!({})));
        assert!(isEmptyObj(Some(&json!({}))));
        assert_eq!(maybeObj(Some(&json!(null))).len(), 0);
        assert!(hasOwn(json!({"x": 1}).as_object().unwrap(), "x"));
        assert_eq!(ensurePresent(Some(7)).unwrap(), 7);
        assert_eq!(validatePositiveInteger("n", &json!(1)).unwrap(), 1.0);
        assert_eq!(coerceInteger(&json!("2")).unwrap(), 2.0);
        assert_eq!(coerceFloat(&json!("2.5")).unwrap(), 2.5);
        assert!(coerceBoolean(&json!("true")));
        assert_eq!(maybeCoerceInteger(Some(&json!("2"))).unwrap(), Some(2.0));
        assert_eq!(maybeCoerceFloat(Some(&json!("2.5"))).unwrap(), Some(2.5));
        assert_eq!(maybeCoerceBoolean(Some(&json!("false"))), Some(false));
        assert_eq!(safeJSON("[]"), Some(json!([])));
    }
}

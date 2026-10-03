//! The JS built-in semantics the identity port reads JSON values and text
//! with: the counterparts of ECMAScript's `ToBoolean`, `String()`,
//! `Number()`, `parseInt`, `encodeURIComponent`, `WhiteSpace`/
//! `LineTerminator`, a template literal's `${}` and `TimeClip`. Not an npm
//! module: every credential, and the MSAL code behind them, runs on these
//! built-ins.

use serde_json::Value;

/// JS truthiness (`ToBoolean`) of a JSON value.
pub(crate) fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number
            .as_f64()
            .is_some_and(|number| number != 0.0 && !number.is_nan()),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// JS truthiness of a `string | undefined`: `x || …`, `if (x)` and `!x` treat
/// the empty string as unset.
pub(crate) trait JsTruthy: Sized {
    fn truthy(self) -> Self;
}

impl JsTruthy for Option<&str> {
    fn truthy(self) -> Self {
        self.filter(|value| !value.is_empty())
    }
}

impl JsTruthy for Option<String> {
    fn truthy(self) -> Self {
        self.filter(|value| !value.is_empty())
    }
}

impl JsTruthy for Option<&std::ffi::OsStr> {
    fn truthy(self) -> Self {
        self.filter(|value| !value.is_empty())
    }
}

/// `String(value)` for a JSON value: objects are `[object Object]`, arrays
/// join their elements with `,` (a `null` element as the empty string), and
/// numbers use `Number.prototype.toString`.
pub(crate) fn js_string(value: &Value) -> String {
    js_string_element(value, false)
}

fn js_string_element(value: &Value, nested_array_element: bool) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(number) => number
            .as_f64()
            .map(number_to_string)
            .unwrap_or_else(|| number.to_string()),
        Value::Bool(value) => value.to_string(),
        Value::Null if nested_array_element => String::new(),
        Value::Null => "null".to_owned(),
        Value::Array(values) => values
            .iter()
            .map(|value| js_string_element(value, true))
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

/// `Number.prototype.toString()` for a finite number: the shortest
/// round-trip digits, with `-0` as `"0"`.
fn number_to_string(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    ryu_js::Buffer::new().format(value).to_owned()
}

/// `Number(text)`: surrounding JS whitespace is ignored and all-whitespace is
/// 0; decimal with sign, fraction and exponent, `0x`/`0o`/`0b`, `Infinity`.
/// `NaN` for anything else.
pub(crate) fn string_to_number(text: &str) -> f64 {
    let value = text.trim_matches(is_js_whitespace);
    if value.is_empty() {
        return 0.0;
    }
    match value {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    for (lower, upper, radix) in [("0x", "0X", 16_u32), ("0b", "0B", 2), ("0o", "0O", 8)] {
        if let Some(digits) = value
            .strip_prefix(lower)
            .or_else(|| value.strip_prefix(upper))
        {
            if digits.is_empty() {
                return f64::NAN;
            }
            let mut number = 0.0;
            for digit in digits.chars() {
                let Some(digit) = digit.to_digit(radix) else {
                    return f64::NAN;
                };
                number = number * f64::from(radix) + f64::from(digit);
            }
            return number;
        }
    }
    let bytes = value.as_bytes();
    let mut index = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let mut has_digit = index > integer_start;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        has_digit |= index > fraction_start;
    }
    if !has_digit {
        return f64::NAN;
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent_start {
            return f64::NAN;
        }
    }
    if index != bytes.len() {
        return f64::NAN;
    }
    value.parse::<f64>().unwrap_or(f64::NAN)
}

/// `parseInt(text, 10)`: leading JS whitespace, an optional sign, then the
/// longest run of decimal digits; `NaN` when there is none.
pub(crate) fn parse_int(text: &str) -> f64 {
    let text = text.trim_start_matches(is_js_whitespace);
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1.0, &text[1..]),
        Some(b'+') => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    let end = digits.bytes().take_while(u8::is_ascii_digit).count();
    if end == 0 {
        return f64::NAN;
    }
    sign * digits[..end].parse::<f64>().unwrap_or(f64::NAN)
}

/// `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )` is
/// percent-encoded as UTF-8.
pub(crate) fn encode_uri_component(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// ECMAScript `WhiteSpace` and `LineTerminator`, which `String.prototype.trim`
/// and `parseInt` skip: `char::is_whitespace` with U+FEFF and without U+0085.
pub(crate) fn is_js_whitespace(ch: char) -> bool {
    (ch.is_whitespace() && ch != '\u{85}') || ch == '\u{feff}'
}

/// A template literal's `${value}`: `String(value)`, `undefined` if missing.
pub(crate) fn template_string(value: Option<&Value>) -> String {
    value.map_or_else(|| "undefined".to_owned(), js_string)
}

/// ECMA-262 `TimeClip`: a time in milliseconds within ±8.64e15, truncated;
/// `None` for anything else (`NaN`, an invalid date).
pub(crate) fn time_clip(time: f64) -> Option<f64> {
    (time.is_finite() && time.abs() <= 8.64e15).then(|| time.trunc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn built_ins_match_ecmascript() {
        assert_eq!(js_string(&json!([1, null, "a"])), "1,,a");
        assert_eq!(js_string(&json!({"a": 1})), "[object Object]");
        assert_eq!(js_string(&json!(1.0)), "1");
        assert_eq!(js_string(&json!(-0.0)), "0");
        assert_eq!(string_to_number(" 0x1F "), 31.0);
        assert_eq!(string_to_number(""), 0.0);
        assert!(string_to_number("1e").is_nan());
        assert_eq!(parse_int(" -12px"), -12.0);
        assert!(parse_int("px").is_nan());
        assert_eq!(encode_uri_component("a b/中"), "a%20b%2F%E4%B8%AD");
    }
}

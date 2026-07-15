// Maps to: TS `BaseAnthropic.stringifyQuery()` in src/client.ts.
//
//! Query-string encoding for primitive query parameters.
//!
//! The TypeScript SDK accepts string/number/boolean/null values, strips
//! `undefined`, and rejects arrays/objects with a helpful error. Rust request
//! query maps used by the client are already typed as optional strings, but
//! this module preserves the TS primitive-stringification behavior for parity
//! tests and for callers that need a lower-level internal helper.

use crate::core::error::ApiError;

/// Primitive query value accepted by TS `stringifyQuery()`.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryValue {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
    Undefined,
}

impl QueryValue {
    fn encode_value(&self) -> Option<String> {
        match self {
            QueryValue::String(value) => Some(value.clone()),
            QueryValue::Number(value) => Some(format_number(*value)),
            QueryValue::Bool(value) => Some(value.to_string()),
            QueryValue::Null => Some(String::new()),
            QueryValue::Undefined => None,
        }
    }
}

impl From<&str> for QueryValue {
    fn from(value: &str) -> Self {
        QueryValue::String(value.to_owned())
    }
}

impl From<String> for QueryValue {
    fn from(value: String) -> Self {
        QueryValue::String(value)
    }
}

impl From<bool> for QueryValue {
    fn from(value: bool) -> Self {
        QueryValue::Bool(value)
    }
}

impl From<i64> for QueryValue {
    fn from(value: i64) -> Self {
        QueryValue::Number(value as f64)
    }
}

impl From<u64> for QueryValue {
    fn from(value: u64) -> Self {
        QueryValue::Number(value as f64)
    }
}

impl From<f64> for QueryValue {
    fn from(value: f64) -> Self {
        QueryValue::Number(value)
    }
}

/// Sentinel for TS `null` query values.
pub fn null() -> QueryValue {
    QueryValue::Null
}

/// Sentinel for TS `undefined` query values.
pub fn undefined() -> QueryValue {
    QueryValue::Undefined
}

/// Maps to: TS `stringifyQuery(query)`.
///
/// The order of the provided pair slice is preserved, matching JS object entry
/// iteration for caller-provided insertion order.
pub fn stringify_query<I, K>(pairs: I) -> Result<String, ApiError>
where
    I: IntoIterator<Item = (K, QueryValue)>,
    K: AsRef<str>,
{
    let mut out = Vec::new();
    for (key, value) in pairs {
        let Some(value) = value.encode_value() else {
            continue;
        };
        out.push(format!(
            "{}={}",
            urlencoding_query_component(key.as_ref()),
            urlencoding_query_component(&value)
        ));
    }
    Ok(out.join("&"))
}

/// Error helper used when callers try to encode nested query parameters.
pub fn unsupported_query_value_error(type_name: &str) -> ApiError {
    ApiError::Sdk(format!(
        "Cannot stringify type {type_name}; Expected string, number, boolean, or null. If you need to pass nested query parameters, you can manually encode them, e.g. {{ query: {{ 'foo[key1]': value1, 'foo[key2]': value2 }} }}, and please open a GitHub issue requesting better support for your use case."
    ))
}

/// Explicitly reject object/array-like values for parity tests and custom
/// callers that discover a non-primitive type dynamically.
pub fn reject_unsupported_query_value(value_type_name: &str) -> ApiError {
    unsupported_query_value_error(value_type_name)
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else if value == 0.0 {
        // JavaScript's String(-0) is "0".
        "0".to_owned()
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn urlencoding_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match *byte {
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
            | b')' => out.push(*byte as char),
            other => {
                out.push('%');
                out.push(hex_digit(other >> 4));
                out.push(hex_digit(other & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'A' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

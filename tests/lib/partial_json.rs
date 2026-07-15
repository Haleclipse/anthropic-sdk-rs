// Mirrors TS SDK tests/lib/partial-json.test.ts.

use anthropic_sdk::vendor::partial_json_parser::partial_parse;
use serde_json::json;

fn parse(input: &str) -> serde_json::Value {
    partial_parse(input).expect("partial JSON should parse best-effort")
}

#[test]
fn partial_parse_valid_complete_json_string() {
    assert_eq!(
        parse(r#"{"foo": "bar", "thing": "baz"}"#),
        json!({"foo": "bar", "thing": "baz"})
    );
}

#[test]
fn partial_parse_valid_partial_json_string() {
    assert_eq!(parse(r#"{"foo": "bar", "thing": ""#), json!({"foo": "bar"}));
}

#[test]
fn partial_parse_empty_json_object() {
    assert_eq!(parse(r#"{}"#), json!({}));
}

#[test]
fn partial_parse_incomplete_nested_json_object() {
    assert_eq!(
        parse(r#"{"foo": {"bar": "baz"}"#),
        json!({"foo": {"bar": "baz"}})
    );
}

#[test]
fn partial_parse_complete_nested_json_object() {
    assert_eq!(
        parse(r#"{"foo": {"bar": "baz"}}"#),
        json!({"foo": {"bar": "baz"}})
    );
}

#[test]
fn partial_parse_json_array_with_incomplete_object() {
    assert_eq!(
        parse(r#"{"foo": [{"bar": "baz"}"#),
        json!({"foo": [{"bar": "baz"}]})
    );
}

#[test]
fn partial_parse_json_array_with_complete_objects() {
    assert_eq!(
        parse(r#"{"foo": [{"bar": "baz"}, {"qux": "quux"}]}"#),
        json!({"foo": [{"bar": "baz"}, {"qux": "quux"}]})
    );
}

#[test]
fn partial_parse_string_with_escaped_characters() {
    assert_eq!(parse(r#"{"foo": "bar\"baz"}"#), json!({"foo": "bar\"baz"}));
}

#[test]
fn partial_parse_string_with_incomplete_escape_sequence() {
    assert_eq!(parse(r#"{"foo": "bar\"#), json!({}));
}

#[test]
fn partial_parse_invalid_json_string_gracefully() {
    assert_eq!(
        parse(r#"{"foo": "bar", "thing": "baz""#),
        json!({"foo": "bar", "thing": "baz"})
    );
}

#[test]
fn partial_parse_json_string_with_null_value() {
    assert_eq!(
        parse(r#"{"foo": null, "bar": "baz"}"#),
        json!({"foo": null, "bar": "baz"})
    );
}

#[test]
fn partial_parse_json_string_with_number_values() {
    assert_eq!(
        parse(r#"{"foo": 123, "bar": 45.67}"#),
        json!({"foo": 123, "bar": 45.67})
    );
}

#[test]
fn partial_parse_json_string_with_boolean_values() {
    assert_eq!(
        parse(r#"{"foo": true, "bar": false}"#),
        json!({"foo": true, "bar": false})
    );
}

#[test]
fn partial_parse_json_string_with_mixed_data_types() {
    assert_eq!(
        parse(r#"{"foo": "bar", "baz": 123, "qux": true, "quux": null}"#),
        json!({"foo": "bar", "baz": 123, "qux": true, "quux": null})
    );
}

#[test]
fn partial_parse_json_string_with_partial_literal_tokens() {
    assert_eq!(parse(r#"{"foo": "bar", "baz": nul"#), json!({"foo": "bar"}));
    assert_eq!(parse(r#"{"foo": "bar", "baz": tr"#), json!({"foo": "bar"}));
    assert_eq!(
        parse(r#"{"foo": "bar", "baz": truee"#),
        json!({"foo": "bar"})
    );
    assert_eq!(parse(r#"{"foo": "bar", "baz": fal"#), json!({"foo": "bar"}));
}

#[test]
fn partial_parse_deeply_nested_json_objects() {
    assert_eq!(
        parse(r#"{"a": {"b": {"c": {"d": "e"}}}}"#),
        json!({"a": {"b": {"c": {"d": "e"}}}})
    );
}

#[test]
fn partial_parse_deeply_nested_partial_json_objects() {
    assert_eq!(
        parse(r#"{"a": {"b": {"c": {"d": "e"#),
        json!({"a": {"b": {"c": {}}}})
    );
}

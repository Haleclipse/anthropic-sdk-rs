// Rust schemars equivalent of TS SDK tests/helpers/zod.test.ts behavior.
//
// Public Rust APIs use schemars + serde instead of Zod. These tests assert the
// equivalent schema-generation and parsing/validation behavior.

use anthropic_sdk::helpers::json_schema::JsonSchemaOutputFormatOptions;
use anthropic_sdk::helpers::{
    schemars_output_config, schemars_output_format, schemars_output_format_with_options,
    schemars_parse_output,
};
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct WeatherOutput {
    city: String,
    temperature: f64,
    conditions: Vec<String>,
}

#[test]
fn schemars_helpers_are_reexported_at_helpers_root_as_rust_zod_equivalents() {
    let format = anthropic_sdk::helpers::schemars_output_format::<WeatherOutput>().unwrap();
    assert_eq!(format.type_name, "json_schema");
}

#[test]
fn schemars_output_format_generates_transformed_json_schema() {
    let format = schemars_output_format::<WeatherOutput>().unwrap();

    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["type"], "object");
    assert_eq!(format.schema["additionalProperties"], false);
    assert!(format.schema["properties"].get("city").is_some());
}

#[test]
fn schemars_output_format_allows_disabling_transform() {
    let format =
        schemars_output_format_with_options::<WeatherOutput>(JsonSchemaOutputFormatOptions {
            transform: false,
        })
        .unwrap();

    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["type"], "object");
    assert!(format.schema.get("additionalProperties").is_none());
}

#[test]
fn schemars_output_config_is_ready_for_message_params() {
    let output_config = schemars_output_config::<WeatherOutput>().unwrap();
    assert!(output_config.effort.is_none());
    assert_eq!(output_config.format.unwrap().type_name, "json_schema");
}

#[test]
fn schemars_parse_output_parses_valid_json() {
    let parsed: WeatherOutput = schemars_parse_output(
        r#"{"city":"San Francisco","temperature":72,"conditions":["sunny","clear"]}"#,
    )
    .unwrap();

    assert_eq!(
        parsed,
        WeatherOutput {
            city: "San Francisco".to_owned(),
            temperature: 72.0,
            conditions: vec!["sunny".to_owned(), "clear".to_owned()],
        }
    );
}

#[test]
fn schemars_parse_output_reports_invalid_json_and_validation_errors() {
    let json_err = schemars_parse_output::<WeatherOutput>("invalid json").unwrap_err();
    assert!(
        json_err
            .to_string()
            .contains("Failed to parse structured output as JSON")
    );

    let validation_err = schemars_parse_output::<WeatherOutput>(r#"{"city":"SF"}"#).unwrap_err();
    assert!(
        validation_err
            .to_string()
            .contains("Failed to parse structured output")
    );
}

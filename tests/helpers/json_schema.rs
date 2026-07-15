// Ported from TS SDK: tests/helpers/json-schema.test.ts

use anthropic_sdk::helpers::json_schema::{
    jsonSchemaOutputFormat, jsonSchemaOutputFormatWithOptions, json_schema_format,
    json_schema_format_with_options, JsonSchemaOutputFormatOptions,
};
use anthropic_sdk::sdk_lib::parser::parse_message;
use anthropic_sdk::{ContentBlock, Message, StopReason, Usage};
use serde_json::json;

#[test]
fn json_schema_format_creates_valid_output_format() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "age": {"type": "number"},
            "active": {"type": "boolean"}
        },
        "required": ["name", "age"]
    });

    let format = json_schema_format(schema.clone()).unwrap();
    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["type"], "object");
    assert_eq!(format.schema["additionalProperties"], false);

    let ts_style = jsonSchemaOutputFormat(schema).unwrap();
    assert_eq!(ts_style.type_name, "json_schema");
    assert_eq!(ts_style.schema["additionalProperties"], false);
}

#[test]
fn json_schema_format_allows_disabling_transform() {
    let schema = json!({
        "type": "object",
        "properties": {"name": {"type": "string"}},
        "additionalProperties": true
    });

    let format =
        json_schema_format_with_options(schema, JsonSchemaOutputFormatOptions { transform: false })
            .unwrap();

    assert_eq!(format.schema["additionalProperties"], true);

    let ts_style = jsonSchemaOutputFormatWithOptions(
        json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "additionalProperties": true
        }),
        JsonSchemaOutputFormatOptions { transform: false },
    )
    .unwrap();
    assert_eq!(ts_style.schema["additionalProperties"], true);
}

#[test]
fn json_schema_format_rejects_non_object_schema() {
    let err = json_schema_format(json!({"type": "string"})).expect_err("should reject string");
    assert!(err
        .to_string()
        .contains("JSON schema must be an object, but got string"));
}

#[test]
fn rust_parser_handles_valid_json_for_schema_helper_use_case() {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Weather {
        city: String,
        temperature: f64,
        conditions: Vec<String>,
    }

    let message = Message {
        id: "msg_123".to_owned(),
        request_id: None,
        content: vec![ContentBlock::Text {
            citations: None,
            text: json!({
                "city": "San Francisco",
                "temperature": 72.0,
                "conditions": ["sunny", "clear"]
            })
            .to_string(),
        }],
        model: "claude-opus-4-6".to_owned(),
        role: "assistant".to_owned(),
        stop_reason: Some(StopReason::EndTurn),
        stop_sequence: None,
        type_name: "message".to_owned(),
        usage: Usage {
            cache_creation: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            inference_geo: None,
            input_tokens: 1,
            output_tokens: 1,
            server_tool_use: None,
            service_tier: None,
        },
    };

    let parsed = parse_message::<Weather>(&message).unwrap();
    assert_eq!(
        parsed.parsed_output.unwrap(),
        Weather {
            city: "San Francisco".to_owned(),
            temperature: 72.0,
            conditions: vec!["sunny".to_owned(), "clear".to_owned()],
        }
    );
}

// Ported from TS SDK: tests/helpers/beta/json-schema.test.ts

use anthropic_sdk::helpers::beta::json_schema::{
    betaJSONSchemaOutputFormat, betaJSONSchemaOutputFormatWithOptions, betaTool,
    beta_json_schema_format, beta_json_schema_format_with_options, beta_json_schema_output_format,
    beta_json_schema_tool, BetaJsonSchemaToolOptions,
};
use anthropic_sdk::helpers::json_schema::JsonSchemaOutputFormatOptions;
use anthropic_sdk::resources::beta::messages::{
    BetaTextBlockParam, BetaToolResultContent, BetaToolResultContentBlockParam,
};
use anthropic_sdk::sdk_lib::tools::RunnableTool;
use serde_json::json;

#[test]
fn beta_json_schema_format_transforms_schema_by_default() {
    let schema = json!({
        "type": "object",
        "properties": {
            "answer": {"type": "string", "minLength": 1}
        },
        "required": ["answer"]
    });

    let format = beta_json_schema_format(schema.clone()).unwrap();
    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["additionalProperties"], false);
    assert_eq!(
        format.schema["properties"]["answer"]["description"],
        "{minLength: 1}"
    );

    let ts_style = betaJSONSchemaOutputFormat(schema).unwrap();
    assert_eq!(ts_style.type_name, "json_schema");
    assert_eq!(ts_style.schema["additionalProperties"], false);
}

#[test]
fn beta_json_schema_format_can_disable_transform() {
    let schema = json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "additionalProperties": true
    });

    let format = beta_json_schema_format_with_options(
        schema,
        JsonSchemaOutputFormatOptions { transform: false },
    )
    .unwrap();

    assert_eq!(format.schema["additionalProperties"], true);

    let ts_style = betaJSONSchemaOutputFormatWithOptions(
        json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "additionalProperties": true
        }),
        JsonSchemaOutputFormatOptions { transform: false },
    )
    .unwrap();
    assert_eq!(ts_style.schema["additionalProperties"], true);
}

#[test]
fn beta_json_schema_output_format_is_ready_for_beta_message_params() {
    let output_config = beta_json_schema_output_format(json!({
        "type": "object",
        "properties": {"answer": {"type": "number"}},
        "required": ["answer"]
    }))
    .unwrap();

    assert!(output_config.effort.is_none());
    let format = output_config.format.unwrap();
    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["additionalProperties"], false);
}

#[tokio::test]
async fn beta_json_schema_tool_generates_definition_and_runs_text_handler() {
    let tool = betaTool(BetaJsonSchemaToolOptions::new_text(
        "lookup",
        json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"]
        }),
        "Lookup weather",
        |input| Ok(format!("weather for {}", input["city"].as_str().unwrap())),
    ))
    .unwrap();

    let definition = tool.definition();
    assert_eq!(definition["type"], "custom");
    assert_eq!(definition["name"], "lookup");
    assert_eq!(definition["input_schema"]["type"], "object");

    let result = tool.run(json!({"city":"Paris"})).await.unwrap();
    assert_eq!(result, "weather for Paris");
}

#[tokio::test]
async fn beta_json_schema_tool_supports_structured_tool_result_content() {
    let tool = beta_json_schema_tool(BetaJsonSchemaToolOptions::new(
        "lookup",
        json!({"type": "object", "properties": {}}),
        "Lookup weather",
        |_input| {
            Ok(BetaToolResultContent::Blocks(vec![
                BetaToolResultContentBlockParam::Text(BetaTextBlockParam {
                    text: "structured".to_owned(),
                    stainless_helpers: Vec::new(),
                    cache_control: None,
                    citations: None,
                }),
            ]))
        },
    ))
    .unwrap();

    match tool.run_beta_tool_result_content(json!({})).await.unwrap() {
        BetaToolResultContent::Blocks(blocks) => assert_eq!(blocks.len(), 1),
        other => panic!("expected structured blocks, got {other:?}"),
    }
}

#[test]
fn beta_json_schema_tool_rejects_non_object_schema_like_ts() {
    let result = beta_json_schema_tool(BetaJsonSchemaToolOptions::new_text(
        "bad",
        json!({"type": "string"}),
        "Bad tool",
        |_input| Ok("never".to_owned()),
    ));
    let err = match result {
        Ok(_) => panic!("expected non-object schema to be rejected"),
        Err(err) => err,
    };

    assert!(err
        .to_string()
        .contains("JSON schema for tool \"bad\" must be an object, but got string"));
}

// Rust schemars equivalent of TS SDK tests/helpers/beta/zod.test.ts behavior.

use anthropic_sdk::helpers::beta::{
    BetaSchemarsToolOptions, beta_schemars_output_config, beta_schemars_output_format,
    beta_schemars_parse_output, beta_schemars_tool,
};
use anthropic_sdk::resources::beta::messages::{
    BetaTextBlockParam, BetaToolResultContent, BetaToolResultContentBlockParam,
};
use anthropic_sdk::sdk_lib::tools::RunnableTool;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
struct WeatherInput {
    location: String,
    unit: Option<String>,
}

#[test]
fn beta_schemars_helpers_are_reexported_at_beta_helpers_root_as_rust_zod_equivalents() {
    let format =
        anthropic_sdk::helpers::beta::beta_schemars_output_format::<WeatherInput>().unwrap();
    assert_eq!(format.type_name, "json_schema");
}

#[test]
fn beta_schemars_output_format_generates_transformed_schema() {
    let format = beta_schemars_output_format::<WeatherInput>().unwrap();

    assert_eq!(format.type_name, "json_schema");
    assert_eq!(format.schema["type"], "object");
    assert_eq!(format.schema["additionalProperties"], false);
    assert!(format.schema["properties"].get("location").is_some());
}

#[test]
fn beta_schemars_output_config_is_ready_for_beta_message_params() {
    let output_config = beta_schemars_output_config::<WeatherInput>().unwrap();
    assert!(output_config.effort.is_none());
    assert_eq!(output_config.format.unwrap().type_name, "json_schema");
}

#[test]
fn beta_schemars_parse_output_uses_serde_validation() {
    let parsed: WeatherInput =
        beta_schemars_parse_output(r#"{"location":"San Francisco","unit":"fahrenheit"}"#).unwrap();

    assert_eq!(
        parsed,
        WeatherInput {
            location: "San Francisco".to_owned(),
            unit: Some("fahrenheit".to_owned()),
        }
    );
}

#[tokio::test]
async fn beta_schemars_tool_generates_definition_and_validates_input() {
    let tool = beta_schemars_tool(BetaSchemarsToolOptions::new(
        "get_weather",
        "Get the weather",
        |input: WeatherInput| Ok(format!("weather in {}", input.location)),
    ))
    .unwrap();

    assert_eq!(tool.name(), "get_weather");
    let definition = tool.definition();
    assert_eq!(definition["type"], "custom");
    assert_eq!(definition["name"], "get_weather");
    assert_eq!(definition["input_schema"]["type"], "object");

    let result = tool
        .run(json!({"location": "San Francisco", "unit": "celsius"}))
        .await
        .unwrap();
    assert_eq!(result, "weather in San Francisco");

    let err = tool
        .run(json!({"unit": "celsius"}))
        .await
        .expect_err("missing required location should fail");
    assert!(err.to_string().contains("Failed to parse tool input"));
}

#[tokio::test]
async fn beta_schemars_tool_supports_structured_tool_result_content_like_ts_zod_tool() {
    let tool = beta_schemars_tool(BetaSchemarsToolOptions::new_content(
        "get_weather",
        "Get the weather",
        |_input: WeatherInput| {
            Ok(BetaToolResultContent::Blocks(vec![
                BetaToolResultContentBlockParam::Text(BetaTextBlockParam {
                    text: "structured weather".to_owned(),
                    stainless_helpers: Vec::new(),
                    cache_control: None,
                    citations: None,
                }),
            ]))
        },
    ))
    .unwrap();

    match tool
        .run_beta_tool_result_content(json!({"location": "San Francisco"}))
        .await
        .unwrap()
    {
        BetaToolResultContent::Blocks(blocks) => assert_eq!(blocks.len(), 1),
        other => panic!("expected structured blocks, got {other:?}"),
    }
}

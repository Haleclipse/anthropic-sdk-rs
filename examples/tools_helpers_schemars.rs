// Rust schemars equivalent of TS examples/tools-helpers-zod.ts
//
// TypeScript uses Zod to define tool schemas and validate model-returned input.
// Rust equivalent: schemars generates JSON Schema and serde deserializes with validation.
//
//   TypeScript Zod helper                 Rust
//   z.object({location: z.string()})      #[derive(JsonSchema, Deserialize)]
//   zodFunction(schema)                   schemars::schema_for!(T)
//   schema.parse(input)                   serde_json::from_value::<T>(input)

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockParam, MessageContent, MessageCreateParams,
    MessageParam, StopReason, Tool, ToolResultBlockParam, ToolResultContent, ToolUnion,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Tool input struct; `#[derive(JsonSchema)]` auto-generates input_schema.
#[derive(Debug, Deserialize, JsonSchema)]
struct GetWeatherInput {
    /// The city and state, e.g. San Francisco, CA
    location: String,
    /// Temperature unit
    #[serde(default = "default_unit")]
    unit: Option<Unit>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Unit {
    Celsius,
    Fahrenheit,
}

fn default_unit() -> Option<Unit> {
    Some(Unit::Celsius)
}

fn get_weather(input: &GetWeatherInput) -> String {
    let unit = match &input.unit {
        Some(Unit::Fahrenheit) => "fahrenheit",
        _ => "celsius",
    };
    format!("The weather in {} is 22 degrees {}.", input.location, unit)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // schemars::schema_for!() generates JSON Schema from Rust types.
    // Equivalent to the schema-generation part of TS zodFunction(z.object({...})).
    let input_schema = serde_json::to_value(schemars::schema_for!(GetWeatherInput))?;

    let tools = vec![ToolUnion::Custom(Tool {
        name: "get_weather".into(),
        description: Some("Get the current weather for a location".into()),
        input_schema,
        cache_control: None,
        eager_input_streaming: None,
        strict: None,
        type_name: None,
    })];

    let response = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text("What is the weather in San Francisco?".into()),
            }],
            tools: Some(tools.clone()),
            ..Default::default()
        })
        .await?;

    assert_eq!(response.stop_reason, Some(StopReason::ToolUse));

    let (tool_use_id, tool_input_raw, tool_name) = response
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse { id, input, name } => {
                Some((id.clone(), input.clone(), name.clone()))
            }
            _ => None,
        })
        .ok_or("Expected a tool_use block")?;

    println!("Tool called: {tool_name}");

    // serde_json::from_value() is Rust's runtime validation/parse step.
    let parsed_input: GetWeatherInput = serde_json::from_value(tool_input_raw)?;
    println!("Parsed input: {parsed_input:?}");

    let tool_result = get_weather(&parsed_input);
    println!("Tool result: {tool_result}");

    let final_response = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![
                MessageParam {
                    role: "user".into(),
                    content: MessageContent::Text("What is the weather in San Francisco?".into()),
                },
                MessageParam {
                    role: "assistant".into(),
                    content: MessageContent::Blocks(
                        response
                            .content
                            .iter()
                            .map(|block| match block {
                                ContentBlock::Text { text, citations } => {
                                    ContentBlockParam::Text(anthropic_sdk::TextBlockParam {
                                        text: text.clone(),
                                        cache_control: None,
                                        citations: citations.as_ref().map(|_| vec![]),
                                        type_name: Some("text".into()),
                                    })
                                }
                                ContentBlock::ToolUse { id, input, name } => {
                                    ContentBlockParam::ToolUse(anthropic_sdk::ToolUseBlockParam {
                                        id: id.clone(),
                                        input: input.clone(),
                                        name: name.clone(),
                                        cache_control: None,
                                    })
                                }
                                _ => unreachable!(),
                            })
                            .collect(),
                    ),
                },
                MessageParam {
                    role: "user".into(),
                    content: MessageContent::Blocks(vec![ContentBlockParam::ToolResult(
                        ToolResultBlockParam {
                            tool_use_id,
                            content: Some(ToolResultContent::Text(tool_result)),
                            cache_control: None,
                            is_error: None,
                        },
                    )]),
                },
            ],
            tools: Some(tools),
            ..Default::default()
        })
        .await?;

    for block in &final_response.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("\nAssistant: {text}");
        }
    }

    Ok(())
}

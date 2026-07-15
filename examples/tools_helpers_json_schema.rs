// Maps to: TS examples/tools-helpers-json-schema.ts
//
// Define tools using JSON schema with serde_json::json! macro. Shows how
// to construct tool definitions inline and parse the model's tool input
// into typed Rust structs using serde deserialization.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockParam, MessageContent, MessageCreateParams,
    MessageParam, StopReason, TextBlockParam, Tool, ToolResultBlockParam, ToolResultContent,
    ToolUnion, ToolUseBlockParam,
};
use serde::Deserialize;

/// Typed struct for the get_weather tool input. Parsed from the model's
/// tool_use JSON via serde.
#[derive(Debug, Deserialize)]
struct GetWeatherInput {
    location: String,
    #[serde(default)]
    unit: Option<String>,
}

/// Typed struct for the get_stock_price tool input.
#[derive(Debug, Deserialize)]
struct GetStockPriceInput {
    symbol: String,
}

/// Build tool definitions using serde_json::json! for the input_schema.
fn build_tools() -> Vec<ToolUnion> {
    vec![
        ToolUnion::Custom(Tool {
            name: "get_weather".into(),
            description: Some("Get the current weather in a given location".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    },
                    "unit": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"],
                        "description": "The temperature unit to use"
                    }
                },
                "required": ["location"]
            }),
            cache_control: None,
            eager_input_streaming: None,
            strict: None,
            type_name: None,
        }),
        ToolUnion::Custom(Tool {
            name: "get_stock_price".into(),
            description: Some("Get the current stock price for a given ticker symbol".into()),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "The stock ticker symbol, e.g. AAPL"
                    }
                },
                "required": ["symbol"]
            }),
            cache_control: None,
            eager_input_streaming: None,
            strict: None,
            type_name: None,
        }),
    ]
}

/// Simulate executing a tool by name, deserializing its input from JSON.
fn execute_tool(name: &str, input: &serde_json::Value) -> Result<String, String> {
    match name {
        "get_weather" => {
            let params: GetWeatherInput =
                serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
            let unit = params.unit.as_deref().unwrap_or("fahrenheit");
            let temp = if unit == "celsius" { 23 } else { 73 };
            Ok(format!(
                "The weather in {} is {}{} and sunny.",
                params.location,
                temp,
                if unit == "celsius" { "C" } else { "F" }
            ))
        }
        "get_stock_price" => {
            let params: GetStockPriceInput =
                serde_json::from_value(input.clone()).map_err(|e| e.to_string())?;
            Ok(format!("The current price of {} is $150.25", params.symbol))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let tools = build_tools();

    let user_message = MessageParam {
        role: "user".into(),
        content: MessageContent::Text(
            "What is the weather in San Francisco and what is Apple's stock price?".into(),
        ),
    };

    // Step 1: Send the initial message with JSON-schema-defined tools
    let message = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![user_message.clone()],
            tools: Some(tools.clone()),
            inference_geo: None,
            metadata: None,
            output_config: None,
            service_tier: None,
            stop_sequences: None,
            stream: None,
            system: None,
            temperature: None,
            thinking: None,
            tool_choice: None,
            top_k: None,
            top_p: None,
        })
        .await?;

    println!("Stop reason: {:?}", message.stop_reason);

    if message.stop_reason != Some(StopReason::ToolUse) {
        println!("No tool use requested.");
        for block in &message.content {
            if let ContentBlock::Text { text, .. } = block {
                println!("{text}");
            }
        }
        return Ok(());
    }

    // Step 2: Execute each tool_use block, deserializing input via serde
    let mut tool_results: Vec<ContentBlockParam> = Vec::new();

    for block in &message.content {
        if let ContentBlock::ToolUse { id, input, name } = block {
            println!("Tool called: {name}");
            println!("  Input JSON: {input}");

            match execute_tool(name, input) {
                Ok(result) => {
                    println!("  Result: {result}");
                    tool_results.push(ContentBlockParam::ToolResult(ToolResultBlockParam {
                        tool_use_id: id.clone(),
                        content: Some(ToolResultContent::Text(result)),
                        cache_control: None,
                        is_error: None,
                    }));
                }
                Err(err) => {
                    println!("  Error: {err}");
                    tool_results.push(ContentBlockParam::ToolResult(ToolResultBlockParam {
                        tool_use_id: id.clone(),
                        content: Some(ToolResultContent::Text(err)),
                        cache_control: None,
                        is_error: Some(true),
                    }));
                }
            }
        }
    }

    // Step 3: Send tool results back for the final response
    let final_response = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![
                user_message,
                MessageParam {
                    role: "assistant".into(),
                    content: MessageContent::Blocks(
                        message
                            .content
                            .iter()
                            .map(|block| match block {
                                ContentBlock::Text { text, citations } => {
                                    ContentBlockParam::Text(TextBlockParam {
                                        text: text.clone(),
                                        cache_control: None,
                                        citations: citations.as_ref().map(|_| vec![]),
                                        type_name: Some("text".into()),
                                    })
                                }
                                ContentBlock::ToolUse { id, input, name } => {
                                    ContentBlockParam::ToolUse(ToolUseBlockParam {
                                        id: id.clone(),
                                        input: input.clone(),
                                        name: name.clone(),
                                        cache_control: None,
                                    })
                                }
                                _ => unreachable!("Unexpected content block type"),
                            })
                            .collect(),
                    ),
                },
                MessageParam {
                    role: "user".into(),
                    content: MessageContent::Blocks(tool_results),
                },
            ],
            tools: Some(tools),
            inference_geo: None,
            metadata: None,
            output_config: None,
            service_tier: None,
            stop_sequences: None,
            stream: None,
            system: None,
            temperature: None,
            thinking: None,
            tool_choice: None,
            top_k: None,
            top_p: None,
        })
        .await?;

    println!("\nFinal response:");
    for block in &final_response.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("{text}");
        }
    }

    Ok(())
}

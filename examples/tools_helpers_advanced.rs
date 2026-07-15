// Maps to: TS examples/tools-helpers-advanced.ts
//
// Automatic tool execution loop using ToolRunner. Defines a tool implementing
// the RunnableTool trait, creates ToolRunnerParams, and calls run_until_done()
// which handles the entire tool-use loop automatically.

use anthropic_sdk::sdk_lib::tools::{RunnableTool, ToolError, ToolRunner, ToolRunnerParams};
use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, MessageContent, MessageCreateParams, MessageParam,
    Tool, ToolUnion,
};

// ---------------------------------------------------------------------------
// Define a tool by implementing RunnableTool
// ---------------------------------------------------------------------------

struct GetWeatherTool;

#[async_trait::async_trait]
impl RunnableTool for GetWeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "get_weather",
            "description": "Get the weather for a specific location",
            "input_schema": {
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. San Francisco, CA"
                    },
                    "unit": {
                        "type": "string",
                        "enum": ["celsius", "fahrenheit"],
                        "description": "The temperature unit"
                    }
                },
                "required": ["location"]
            }
        })
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        let location = input
            .get("location")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing required parameter: location"))?;

        // Simulated weather lookup
        let result = serde_json::json!({
            "location": location,
            "temperature": 73,
            "unit": "fahrenheit",
            "conditions": "sunny",
            "humidity": "45%"
        });

        Ok(result.to_string())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // Build the tool list for the API request. The ToolUnion wraps the JSON
    // schema; the RunnableTool handles execution.
    let tools = vec![ToolUnion::Custom(Tool {
        name: "get_weather".into(),
        description: Some("Get the weather for a specific location".into()),
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
                    "description": "The temperature unit"
                }
            },
            "required": ["location"]
        }),
        cache_control: None,
        eager_input_streaming: None,
        strict: None,
        type_name: None,
    })];

    let params = ToolRunnerParams {
        create_params: MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text(
                    "What is the weather in San Francisco and New York?".into(),
                ),
            }],
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
        },
        tools: vec![Box::new(GetWeatherTool)],
        max_iterations: Some(5),
    };

    // run_until_done() drives the full agentic loop:
    //   send message -> execute tools -> send results -> repeat
    // until the model produces a final text response or max_iterations is hit.
    let mut runner = ToolRunner::new(&client, params);
    let final_message = runner.run_until_done().await?;

    println!("Final response:");
    for block in &final_message.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("{text}");
        }
    }

    println!("\nStop reason: {:?}", final_message.stop_reason);
    println!("Usage: {:?}", final_message.usage);

    Ok(())
}

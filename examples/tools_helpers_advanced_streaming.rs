// Maps to: TS examples/tools-helpers-advanced-streaming.ts
//
// Tool execution with streaming output. In the TS SDK, the tool runner supports
// both streaming and non-streaming modes. In Rust, ToolRunner currently drives
// a non-streaming loop. This example shows the pattern: use ToolRunner for the
// automatic tool loop, then stream the final turn for incremental output.

use anthropic_sdk::sdk_lib::tools::{RunnableTool, ToolError, ToolRunner, ToolRunnerParams};
use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockDelta, MessageContent, MessageCreateParams,
    MessageParam, MessageStreamEvent, StopReason, Tool, ToolUnion,
};
use futures::StreamExt;

// ---------------------------------------------------------------------------
// Tool definition
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

        Ok(format!("The weather in {} is 72F and sunny.", location))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let tools = vec![ToolUnion::Custom(Tool {
        name: "get_weather".into(),
        description: Some("Get the weather for a specific location".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                }
            },
            "required": ["location"]
        }),
        cache_control: None,
        eager_input_streaming: None,
        strict: None,
        type_name: None,
    })];

    // Phase 1: Use ToolRunner to drive the tool execution loop.
    // This handles all tool calls non-streaming until the model is ready
    // to produce its final answer.
    let params = ToolRunnerParams {
        create_params: MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text("What is the weather in San Francisco?".into()),
            }],
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
        },
        tools: vec![Box::new(GetWeatherTool)],
        max_iterations: Some(5),
    };

    let mut runner = ToolRunner::new(&client, params);
    let result = runner.run_until_done().await?;

    // If the runner already produced a final text response, print it.
    if result.stop_reason != Some(StopReason::ToolUse) {
        println!("Tool runner completed. Final response:");
        for block in &result.content {
            if let ContentBlock::Text { text, .. } = block {
                println!("{text}");
            }
        }
    }

    // Phase 2: Alternatively, after the tool loop resolves, you can stream
    // a follow-up request using the accumulated conversation history.
    // This gives you incremental text output for the final answer.
    println!("\n--- Streaming a follow-up question ---\n");

    let follow_up_params = MessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 1024,
        messages: vec![MessageParam {
            role: "user".into(),
            content: MessageContent::Text(
                "Based on the weather, should I bring a jacket to San Francisco?".into(),
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
    };

    let mut stream = client.messages().stream(&follow_up_params).await?;

    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } => {
                print!("{text}");
            }
            MessageStreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    let final_msg = stream.final_message().await?;
    println!("\nStop reason: {:?}", final_msg.stop_reason);

    Ok(())
}

// Maps to: TS examples/tools.ts
//
// Tool use with manual tool execution. Defines a get_weather tool, sends a
// message, handles the tool_use response, and sends the tool_result back.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockParam, MessageContent, MessageCreateParams,
    MessageParam, StopReason, Tool, ToolResultBlockParam, ToolResultContent, ToolUnion,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let user_message = MessageParam {
        role: "user".into(),
        content: MessageContent::Text("What is the weather in SF?".into()),
    };

    let tools = vec![ToolUnion::Custom(Tool {
        name: "get_weather".into(),
        description: Some("Get the weather for a specific location".into()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            }
        }),
        cache_control: None,
        eager_input_streaming: None,
        strict: None,
        type_name: None,
    })];

    // Step 1: Send the initial message with tools
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

    println!("Initial response:");
    println!("{message:#?}");

    assert_eq!(message.stop_reason, Some(StopReason::ToolUse));

    // Step 2: Find the tool_use block
    let tool_use = message
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse { id, input, name } => Some((id, input, name)),
            _ => None,
        })
        .expect("Expected a tool_use block in the response");

    println!("\nTool called: {} with input: {}", tool_use.2, tool_use.1);

    // Step 3: Send the tool result back
    let result = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![
                user_message,
                // The assistant's response (including tool_use)
                MessageParam {
                    role: "assistant".into(),
                    content: MessageContent::Blocks(
                        message
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
                                _ => unreachable!("Unexpected content block type"),
                            })
                            .collect(),
                    ),
                },
                // The tool result
                MessageParam {
                    role: "user".into(),
                    content: MessageContent::Blocks(vec![ContentBlockParam::ToolResult(
                        ToolResultBlockParam {
                            tool_use_id: tool_use.0.clone(),
                            content: Some(ToolResultContent::Text("The weather is 73f".into())),
                            cache_control: None,
                            is_error: None,
                        },
                    )]),
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
    println!("{result:#?}");

    Ok(())
}

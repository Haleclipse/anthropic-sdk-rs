// Maps to: TS examples/tools-streaming.ts
//
// Tool use with streaming. Sends a message with tools defined, streams the
// response, handles content_block_start for tool_use blocks, accumulates
// input_json_delta fragments, then sends the tool_result back.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockDelta, ContentBlockParam, MessageContent,
    MessageCreateParams, MessageParam, MessageStreamEvent, StopReason, TextBlockParam, Tool,
    ToolResultBlockParam, ToolResultContent, ToolUnion, ToolUseBlockParam,
};
use futures::StreamExt;

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

    let user_message = MessageParam {
        role: "user".into(),
        content: MessageContent::Text("What is the weather in SF?".into()),
    };

    let params = MessageCreateParams {
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
    };

    // Step 1: Stream the initial response and collect tool_use blocks
    let mut stream = client.messages().stream(&params).await?;

    // Track the current tool_use block being streamed
    let mut _current_tool_name: Option<String> = None;
    let mut current_tool_id: Option<String> = None;
    let mut input_json_parts: Vec<String> = Vec::new();
    let mut assistant_content_blocks: Vec<ContentBlock> = Vec::new();

    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockStart { content_block, .. } => {
                match &content_block {
                    ContentBlock::ToolUse { id, name, .. } => {
                        println!("Tool use started: {name} (id: {id})");
                        _current_tool_name = Some(name.clone());
                        current_tool_id = Some(id.clone());
                        input_json_parts.clear();
                    }
                    ContentBlock::Text { .. } => {
                        print!("Text: ");
                    }
                    _ => {}
                }
                assistant_content_blocks.push(content_block);
            }
            MessageStreamEvent::ContentBlockDelta { delta, index } => match delta {
                ContentBlockDelta::TextDelta { text } => {
                    print!("{text}");
                    // Update the text in our tracked blocks
                    if let Some(ContentBlock::Text {
                        text: ref mut t, ..
                    }) = assistant_content_blocks.get_mut(index)
                    {
                        t.push_str(&text);
                    }
                }
                ContentBlockDelta::InputJsonDelta { partial_json } => {
                    input_json_parts.push(partial_json);
                }
                _ => {}
            },
            MessageStreamEvent::ContentBlockStop { index } => {
                // If this was a tool_use block, finalize its input JSON
                if current_tool_id.is_some() {
                    let full_json: String = input_json_parts.join("");
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&full_json) {
                        if let Some(ContentBlock::ToolUse { ref mut input, .. }) =
                            assistant_content_blocks.get_mut(index)
                        {
                            *input = parsed;
                        }
                    }
                    println!("\nTool input accumulated: {}", input_json_parts.join(""));
                }
            }
            MessageStreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    let final_msg = stream.final_message().await?;

    // If the model did not request a tool call, we are done
    if final_msg.stop_reason != Some(StopReason::ToolUse) {
        println!("No tool use requested. Done.");
        return Ok(());
    }

    // Step 2: Find the tool_use block from the final assembled message
    let tool_use = final_msg
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::ToolUse { id, input, name } => Some((id, input, name)),
            _ => None,
        })
        .ok_or("Expected a tool_use block in the response")?;

    println!("Tool called: {} with input: {}", tool_use.2, tool_use.1);

    // Step 3: Send the tool result back (non-streaming for the final answer)
    let result = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![
                user_message,
                // The assistant's response containing tool_use
                MessageParam {
                    role: "assistant".into(),
                    content: MessageContent::Blocks(
                        final_msg
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
    for block in &result.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("{text}");
        }
    }

    Ok(())
}

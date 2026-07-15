// Maps to: TS examples/web-search-stream.ts
//
// Web search with streaming. Uses the web_search_20250305 server tool and
// streams the response, printing text deltas and content block starts as
// they arrive.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, ContentBlockDelta, MessageContent, MessageCreateParams,
    MessageParam, MessageStreamEvent, ToolUnion, WebSearchTool20250305Typed,
    WebSearchToolResultBlockContent,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let params = MessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 1024,
        messages: vec![MessageParam {
            role: "user".into(),
            content: MessageContent::Text(
                "What is the latest version of the Rust programming language?".into(),
            ),
        }],
        tools: Some(vec![ToolUnion::WebSearch(WebSearchTool20250305Typed {
            type_name: "web_search_20250305".into(),
            name: "web_search".into(),
            ..Default::default()
        })]),
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

    let mut stream = client.messages().stream(&params).await?;

    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockStart { content_block, .. } => match &content_block {
                ContentBlock::Text { .. } => {
                    println!("[text block started]");
                }
                ContentBlock::ServerToolUse { name, .. } => {
                    println!("[server tool use: {name}]");
                }
                ContentBlock::WebSearchToolResult { content, .. } => match content {
                    WebSearchToolResultBlockContent::Results(results) => {
                        println!("[web search results: {} items]", results.len());
                        for result in results {
                            println!("  {} ({})", result.title, result.url);
                        }
                    }
                    WebSearchToolResultBlockContent::Error(err) => {
                        println!("[web search error: {}]", err.error_code);
                    }
                },
                _ => {}
            },
            MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } => print!("{text}"),
            MessageStreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    let final_msg = stream.final_message().await?;
    println!("\nStop reason: {:?}", final_msg.stop_reason);
    println!("Usage: {:?}", final_msg.usage);

    Ok(())
}

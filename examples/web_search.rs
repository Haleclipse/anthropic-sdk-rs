// Maps to: TS examples/web-search.ts
//
// Web search using the server-side web_search_20250305 tool. Sends a query
// that requires up-to-date information, then prints text and search results.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, MessageContent, MessageCreateParams, MessageParam,
    ToolUnion, WebSearchTool20250305Typed, WebSearchToolResultBlockContent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let message = client
        .messages()
        .create(&MessageCreateParams {
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
        })
        .await?;

    for block in &message.content {
        match block {
            ContentBlock::Text { text, .. } => {
                println!("{text}");
            }
            ContentBlock::WebSearchToolResult { content, .. } => match content {
                WebSearchToolResultBlockContent::Results(results) => {
                    println!("\n--- Web Search Results ---");
                    for result in results {
                        println!("  {} ({})", result.title, result.url);
                    }
                }
                WebSearchToolResultBlockContent::Error(err) => {
                    println!("Search error: {}", err.error_code);
                }
            },
            _ => {}
        }
    }

    Ok(())
}

// Maps to: TS examples/streaming.ts
//
// High-level streaming with MessageStream. Iterates over events, printing
// text deltas as they arrive, then retrieves the final assembled message.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlockDelta, MessageContent, MessageCreateParams, MessageParam,
    MessageStreamEvent,
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
                "How can I recursively list all files in a directory in Rust?".into(),
            ),
        }],
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
        tools: None,
        top_k: None,
        top_p: None,
    };

    let mut stream = client.messages().stream(&params).await?;

    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } => print!("{text}"),
            MessageStreamEvent::ContentBlockStart { content_block, .. } => {
                println!("contentBlock: {content_block:?}");
            }
            MessageStreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    // After the stream is fully consumed we can get the final assembled message.
    let final_msg = stream.final_message().await?;
    println!("\nStop reason: {:?}", final_msg.stop_reason);
    println!("Usage: {:?}", final_msg.usage);

    Ok(())
}

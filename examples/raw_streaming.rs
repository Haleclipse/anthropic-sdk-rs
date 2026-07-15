// Maps to: TS examples/raw-streaming.ts
//
// Low-level SSE streaming using create_stream() which returns the raw
// SseStream<MessageStreamEvent> directly, without the higher-level
// MessageStream wrapper and its automatic delta accumulation.

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
        max_tokens: 500,
        messages: vec![MessageParam {
            role: "user".into(),
            content: MessageContent::Text("Hey Claude!".into()),
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

    // create_stream() returns SseStream<MessageStreamEvent> -- a raw SSE
    // event stream with no automatic accumulation.
    let mut stream = client.messages().create_stream(&params).await?;

    while let Some(event) = stream.next().await {
        let event = event?;
        if let MessageStreamEvent::ContentBlockDelta {
            delta: ContentBlockDelta::TextDelta { text },
            ..
        } = event
        {
            print!("{text}");
        }
    }

    println!();

    Ok(())
}

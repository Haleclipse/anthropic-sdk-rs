// Maps to: TS examples/thinking-stream.ts
//
// Extended thinking with streaming. Enables the model's internal
// chain-of-thought, then streams both thinking deltas and text deltas,
// printing them with distinct prefixes.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlockDelta, MessageContent, MessageCreateParams, MessageParam,
    MessageStreamEvent, ThinkingConfig,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let params = MessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 3200,
        messages: vec![MessageParam {
            role: "user".into(),
            content: MessageContent::Text("Create a haiku about Anthropic.".into()),
        }],
        thinking: Some(ThinkingConfig::Enabled {
            budget_tokens: 1600,
        }),
        inference_geo: None,
        metadata: None,
        output_config: None,
        service_tier: None,
        stop_sequences: None,
        stream: None,
        system: None,
        temperature: None,
        tool_choice: None,
        tools: None,
        top_k: None,
        top_p: None,
    };

    let mut stream = client.messages().stream(&params).await?;
    let mut in_thinking = false;

    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockDelta { delta, .. } => match delta {
                ContentBlockDelta::ThinkingDelta { thinking } => {
                    if !in_thinking {
                        println!("[thinking]");
                        in_thinking = true;
                    }
                    print!("{thinking}");
                }
                ContentBlockDelta::TextDelta { text } => {
                    if in_thinking {
                        println!("\n[/thinking]\n");
                        in_thinking = false;
                    }
                    print!("{text}");
                }
                _ => {}
            },
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

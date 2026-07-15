// Maps to: TS examples/thinking.ts
//
// Extended thinking: enables the model's internal chain-of-thought with a
// token budget, then prints both thinking and text blocks from the response.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, MessageContent, MessageCreateParams, MessageParam,
    ThinkingConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let message = client
        .messages()
        .create(&MessageCreateParams {
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
        })
        .await?;

    for block in &message.content {
        match block {
            ContentBlock::Thinking { thinking, .. } => {
                println!("Thinking: {thinking}");
            }
            ContentBlock::Text { text, .. } => {
                println!("Text: {text}");
            }
            _ => {}
        }
    }

    Ok(())
}

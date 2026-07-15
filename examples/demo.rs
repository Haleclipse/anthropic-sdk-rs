// Maps to: TS examples/demo.ts
//
// Basic non-streaming message creation. Gets the API key from the
// ANTHROPIC_API_KEY environment variable.

use anthropic_sdk::{Anthropic, ClientOptions, MessageContent, MessageCreateParams, MessageParam};

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
                content: MessageContent::Text("Hey Claude!".into()),
            }],
            // Optional fields
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
        })
        .await?;

    println!("{message:#?}");

    Ok(())
}

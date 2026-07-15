// Maps to: TS examples/structured-outputs-streaming.ts
//
// Structured JSON output with streaming. Uses output_config to constrain the
// model to JSON, then streams text deltas and accumulates them. The final
// accumulated text is parsed as JSON at the end.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlockDelta, JsonOutputFormat, MessageContent,
    MessageCreateParams, MessageParam, MessageStreamEvent, OutputConfig,
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
                "List 3 popular programming languages with their creators. Return as JSON.".into(),
            ),
        }],
        output_config: Some(OutputConfig {
            format: Some(JsonOutputFormat {
                schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "languages": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string" },
                                    "creator": { "type": "string" },
                                    "year": { "type": "integer" }
                                },
                                "required": ["name", "creator", "year"]
                            }
                        }
                    },
                    "required": ["languages"]
                }),
                type_name: "json_schema".into(),
            }),
            effort: None,
        }),
        inference_geo: None,
        metadata: None,
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
    let mut accumulated = String::new();

    println!("Streaming JSON deltas:");
    while let Some(event) = stream.next().await {
        match event? {
            MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } => {
                print!("{text}");
                accumulated.push_str(&text);
            }
            MessageStreamEvent::MessageStop => {
                println!();
            }
            _ => {}
        }
    }

    // Parse the accumulated JSON.
    let parsed: serde_json::Value = serde_json::from_str(&accumulated)?;

    println!("\nParsed result:");
    if let Some(languages) = parsed["languages"].as_array() {
        for lang in languages {
            println!(
                "  {} - created by {} in {}",
                lang["name"], lang["creator"], lang["year"]
            );
        }
    }

    Ok(())
}

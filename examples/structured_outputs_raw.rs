// Maps to: TS examples/structured-outputs-raw.ts
//
// Structured JSON output using output_config with an inline JSON schema.
// The model is constrained to produce valid JSON matching the schema.
// We parse the text response manually.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, JsonOutputFormat, MessageContent, MessageCreateParams,
    MessageParam, OutputConfig,
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
                    "What is the capital of France? Reply with a JSON object.".into(),
                ),
            }],
            output_config: Some(OutputConfig {
                format: Some(JsonOutputFormat {
                    schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "country": { "type": "string", "description": "The country name" },
                            "capital": { "type": "string", "description": "The capital city" }
                        },
                        "required": ["country", "capital"]
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
        })
        .await?;

    // Extract the text content and parse it as JSON.
    let text = message
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .ok_or("No text block in response")?;

    let parsed: serde_json::Value = serde_json::from_str(text)?;

    println!("Raw text: {text}");
    println!("Parsed JSON: {parsed:#}");
    println!("Country: {}", parsed["country"]);
    println!("Capital: {}", parsed["capital"]);

    Ok(())
}

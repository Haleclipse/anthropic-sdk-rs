// Maps to: TS examples/structured-outputs-json-schema.ts
//
// Structured JSON output using the helpers::json_schema module.
// This uses the json_schema_output_format helper to build the OutputConfig
// from a serde_json::json! schema, rather than constructing it manually.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, MessageContent, MessageCreateParams, MessageParam,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "explanation": { "type": "string" },
                        "output": { "type": "string" }
                    },
                    "required": ["explanation", "output"]
                }
            },
            "final_answer": { "type": "string" }
        },
        "required": ["steps", "final_answer"]
    });

    let output_config = anthropic_sdk::helpers::json_schema::json_schema_output_format(schema)?;

    let message = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text(
                    "What is 27 * 453? Show your work step by step.".into(),
                ),
            }],
            output_config: Some(output_config),
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

    println!("Parsed response:");
    println!("{parsed:#}");

    if let Some(steps) = parsed["steps"].as_array() {
        for (i, step) in steps.iter().enumerate() {
            println!(
                "Step {}: {} -> {}",
                i + 1,
                step["explanation"],
                step["output"]
            );
        }
    }
    println!("Final answer: {}", parsed["final_answer"]);

    Ok(())
}

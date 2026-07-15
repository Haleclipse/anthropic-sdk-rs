// Maps to: TS examples/tools-helpers-memory.ts
//
// Memory tools are a beta feature that allows the model to persist and recall
// information across conversations. This example demonstrates how to configure
// a memory tool via BetaMessageCreateParams using the beta Messages API.
//
// Memory tools are server-managed — the API handles storage and retrieval.
// You simply declare the tool and the model decides when to use it.
//
// Note: This requires beta API access with the appropriate feature flag.

use anthropic_sdk::resources::beta::messages::types::{
    BetaContentBlock, BetaMemoryTool20250818, BetaMessageContent, BetaMessageCreateParams,
    BetaMessageParam, BetaToolUnion,
};
use anthropic_sdk::resources::beta::types::Beta;
use anthropic_sdk::{Anthropic, ClientOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // Define a memory tool as a server-managed tool via the tools array.
    let memory_tool = BetaToolUnion::Memory20250818(BetaMemoryTool20250818::default());

    let params = BetaMessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 1024,
        messages: vec![BetaMessageParam {
            role: "user".into(),
            content: BetaMessageContent::Text(
                "My favorite color is blue and I live in Portland, Oregon. \
                     Please remember this for future conversations."
                    .into(),
            ),
        }],
        betas: Some(vec![
            // Memory tools are part of the broader beta features.
            // Use the appropriate beta version string for your API access.
            "interleaved-thinking-2025-05-14".into(),
        ]),
        tools: Some(vec![memory_tool]),
        // Optional fields
        container: None,
        context_management: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: None,
        output_format: None,
        service_tier: None,
        speed: None,
        stop_sequences: None,
        stream: None,
        system: None,
        temperature: None,
        thinking: None,
        tool_choice: None,
        top_k: None,
        top_p: None,
    };

    let beta = Beta::new(&client);
    let message = beta.messages().create(&params).await?;

    println!("Response ID: {}", message.id);
    println!("Stop reason: {:?}", message.stop_reason);

    for (i, block) in message.content.iter().enumerate() {
        match block {
            BetaContentBlock::Text { text, .. } => {
                println!("\n[Block {i}] Text: {text}");
            }
            BetaContentBlock::ToolUse { name, input, .. } => {
                println!("\n[Block {i}] Tool Use: {name}");
                println!("  Input: {input}");
            }
            BetaContentBlock::ServerToolUse { name, input, .. } => {
                println!("\n[Block {i}] Server Tool Use: {name:?}");
                println!("  Input: {input:?}");
            }
            other => {
                println!("\n[Block {i}] {other:?}");
            }
        }
    }

    println!(
        "\nUsage: input={}, output={}",
        message.usage.input_tokens, message.usage.output_tokens
    );

    Ok(())
}

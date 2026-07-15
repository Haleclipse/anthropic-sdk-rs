// Maps to: TS examples/autoCompaction.ts
//
// Demonstrates automatic context compaction via the beta Messages API.
// When a conversation grows too long, the API can automatically compact
// earlier turns into a summary so the model stays within its context window.
//
// This example:
//   1. Builds a long conversation history to approach the context limit.
//   2. Sends it with context_management enabled (beta feature).
//   3. Inspects the response for compaction blocks and stop_reason.

use anthropic_sdk::resources::beta::messages::types::{
    BetaContentBlock, BetaContextManagementConfig, BetaMessageContent, BetaMessageCreateParams,
    BetaMessageParam, BetaStopReason,
};
use anthropic_sdk::resources::beta::types::Beta;
use anthropic_sdk::{Anthropic, ClientOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // Build a long conversation to simulate approaching the context limit.
    // In practice you would accumulate real conversation turns over time.
    let mut messages = Vec::new();

    for i in 0..20 {
        messages.push(BetaMessageParam {
            role: "user".into(),
            content: BetaMessageContent::Text(format!(
                "This is turn {i}. Tell me an interesting fact about the number {i}."
            )),
        });
        messages.push(BetaMessageParam {
            role: "assistant".into(),
            content: BetaMessageContent::Text(format!(
                "The number {i} is interesting because it appears in many mathematical contexts."
            )),
        });
    }

    // Add the final user message.
    messages.push(BetaMessageParam {
        role: "user".into(),
        content: BetaMessageContent::Text("Summarize what we have discussed so far.".into()),
    });

    let params = BetaMessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 1024,
        messages,
        betas: Some(vec!["context-management-2025-06-27".into()]),
        // Enable context management so the API may compact earlier turns.
        context_management: Some(BetaContextManagementConfig { edits: None }),
        // Optional fields
        container: None,
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
        tools: None,
        top_k: None,
        top_p: None,
    };

    let beta = Beta::new(&client);
    let message = beta.messages().create(&params).await?;

    println!("Response ID: {}", message.id);
    println!("Stop reason: {:?}", message.stop_reason);

    // Check if auto-compaction was triggered.
    if message.stop_reason == Some(BetaStopReason::Compaction) {
        println!("\n** Auto-compaction was triggered! **");
    }

    // Check for context management information.
    if let Some(ref ctx) = message.context_management {
        println!(
            "Context management edits applied: {}",
            ctx.applied_edits.len()
        );
    }

    // Print content blocks, looking for compaction blocks.
    for (i, block) in message.content.iter().enumerate() {
        match block {
            BetaContentBlock::Compaction { content } => {
                println!("\n[Block {i}] Compaction:");
                match content {
                    Some(summary) => {
                        let preview: String = summary.chars().take(200).collect();
                        println!("  Summary: {preview}...");
                    }
                    None => {
                        println!("  (compaction failed — no summary produced)");
                    }
                }
            }
            BetaContentBlock::Text { text, .. } => {
                println!("\n[Block {i}] Text:");
                let preview: String = text.chars().take(200).collect();
                println!("  {preview}...");
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

// Maps to: TS examples/mcp.ts
//
// MCP (Model Context Protocol) server tools via the beta Messages API.
// Demonstrates how to configure MCP servers in a BetaMessageCreateParams
// request so the model can discover and invoke tools hosted on external
// MCP servers during inference.
//
// Requirements:
//   - An MCP server reachable at the URL below (replace with your own).
//   - The beta feature "mcp-client-2025-04-04" or later.

use anthropic_sdk::resources::beta::messages::types::{
    BetaContentBlock, BetaMCPServerDefinition, BetaMessageContent, BetaMessageCreateParams,
    BetaMessageParam, BetaRequestMCPServerToolConfiguration,
};
use anthropic_sdk::resources::beta::types::Beta;
use anthropic_sdk::{Anthropic, ClientOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // Configure an MCP server that the model can call during inference.
    // Replace the URL and name with your own MCP server details.
    let mcp_servers = vec![BetaMCPServerDefinition {
        name: "my-mcp-server".into(),
        type_name: "url".into(),
        url: "https://example.com/mcp".into(),
        authorization_token: None,
        tool_configuration: Some(BetaRequestMCPServerToolConfiguration {
            allowed_tools: Some(vec!["get_weather".into()]),
            enabled: None,
        }),
    }];

    let params = BetaMessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 1024,
        messages: vec![BetaMessageParam {
            role: "user".into(),
            content: BetaMessageContent::Text("What is the weather in San Francisco?".into()),
        }],
        betas: Some(vec!["mcp-client-2025-04-04".into()]),
        mcp_servers: Some(mcp_servers),
        // Optional fields
        container: None,
        context_management: None,
        inference_geo: None,
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

    // Use the beta messages resource to create the request.
    let beta = Beta::new(&client);
    let message = beta.messages().create(&params).await?;

    println!("Response ID: {}", message.id);
    println!("Model: {}", message.model);
    println!("Stop reason: {:?}", message.stop_reason);

    for block in &message.content {
        match block {
            BetaContentBlock::Text { text, .. } => {
                println!("\nText: {text}");
            }
            BetaContentBlock::McpToolUse {
                name,
                server_name,
                input,
                ..
            } => {
                println!("\nMCP Tool Use: {name} (server: {server_name})\n  Input: {input}");
            }
            other => {
                println!("\nBlock: {other:?}");
            }
        }
    }

    Ok(())
}

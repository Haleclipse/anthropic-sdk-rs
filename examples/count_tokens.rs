// Maps to: TS examples/count-tokens.ts
//
// Token counting: returns the number of input tokens the given request
// would consume, without actually creating a message.

use anthropic_sdk::{
    Anthropic, ClientOptions, MessageContent, MessageCountTokensParams, MessageParam,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let result = client
        .messages()
        .count_tokens(&MessageCountTokensParams {
            model: "claude-sonnet-4-5-20250929".into(),
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text("Hey Claude!".into()),
            }],
            output_config: None,
            system: None,
            thinking: None,
            tool_choice: None,
            tools: None,
        })
        .await?;

    println!("{result:#?}");

    Ok(())
}

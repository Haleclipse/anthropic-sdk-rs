// Maps to: TS examples/cancellation.ts
//
// Demonstrates two ways of cancelling a streaming request:
//
// 1. Breaking out of the event loop (the stream is dropped, closing the
//    connection).
// 2. Using tokio::time::timeout to abort after a deadline.
//
// The example races to see whether some Rust code prints "unwrap" before
// 1.5 seconds elapse.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlockDelta, MessageContent, MessageCreateParams, MessageParam,
    MessageStreamEvent,
};
use futures::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    let question = "How can I recursively list all files in a directory in Rust?";

    let params = MessageCreateParams {
        model: "claude-sonnet-4-5-20250929".into(),
        max_tokens: 500,
        messages: vec![MessageParam {
            role: "user".into(),
            content: MessageContent::Text(question.into()),
        }],
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
    };

    let mut stream = client.messages().create_stream(&params).await?;

    // Method 2: cancel from outside the loop via tokio timeout.
    // We wrap the entire consumption loop in a timeout.
    let result = tokio::time::timeout(Duration::from_millis(1500), async {
        while let Some(event) = stream.next().await {
            let event = event?;
            if let MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text },
                ..
            } = &event
            {
                print!("{text}");

                // Method 1: cancel by breaking out of the loop.
                if text.contains("unwrap") {
                    println!("\nCancelling after seeing \"unwrap\".");
                    return Ok::<bool, Box<dyn std::error::Error>>(true);
                }
            }
        }
        Ok(false)
    })
    .await;

    match result {
        Ok(Ok(true)) => {
            // Cancelled via break (found "unwrap")
        }
        Ok(Ok(false)) => {
            println!("\nStream completed without seeing \"unwrap\".");
        }
        Ok(Err(e)) => {
            return Err(e);
        }
        Err(_) => {
            println!("\nCancelling after 1.5 seconds.");
            // The stream is dropped here, closing the connection.
        }
    }

    Ok(())
}

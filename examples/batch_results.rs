// Maps to: TS examples/batch-results.ts
//
// Retrieve and print the results of a completed Message Batch. Takes a batch
// ID from the command-line arguments and calls the batches results API.

use anthropic_sdk::Anthropic;
use anthropic_sdk::ClientOptions;
use futures::StreamExt;

// The Batches resource lives under the messages module.
use anthropic_sdk::resources::messages::batches::Batches;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let batch_id = std::env::args()
        .nth(1)
        .expect("usage: batch_results <batch_id>");

    let client = Anthropic::new(ClientOptions::default())?;

    // Construct the Batches resource from the client.
    let batches = Batches::new(&client);

    // Retrieve the batch to show its status first.
    let batch = batches.retrieve(&batch_id).await?;
    println!("Batch: {}", batch.id);
    println!("Status: {:?}", batch.processing_status);
    println!(
        "Requests — succeeded: {}, errored: {}, canceled: {}, expired: {}, processing: {}",
        batch.request_counts.succeeded,
        batch.request_counts.errored,
        batch.request_counts.canceled,
        batch.request_counts.expired,
        batch.request_counts.processing,
    );

    // Stream the results (only available once the batch has finished processing).
    let mut results = batches.results(&batch_id).await?;

    println!("\n--- Results ---\n");

    while let Some(item) = results.next().await {
        let item = item?;
        println!("custom_id: {}", item.custom_id);
        match &item.result {
            anthropic_sdk::resources::messages::batches::MessageBatchResult::Succeeded {
                message,
            } => {
                println!("  status: succeeded");
                println!("  stop_reason: {:?}", message.stop_reason);
                for block in &message.content {
                    match block {
                        anthropic_sdk::ContentBlock::Text { text, .. } => {
                            // Print a truncated preview of the text.
                            let preview: String = text.chars().take(120).collect();
                            println!("  text: {preview}...");
                        }
                        _ => {
                            println!("  block: {block:?}");
                        }
                    }
                }
            }
            anthropic_sdk::resources::messages::batches::MessageBatchResult::Errored { error } => {
                println!("  status: errored — {}", error.error.message());
            }
            anthropic_sdk::resources::messages::batches::MessageBatchResult::Canceled => {
                println!("  status: canceled");
            }
            anthropic_sdk::resources::messages::batches::MessageBatchResult::Expired => {
                println!("  status: expired");
            }
        }
        println!();
    }

    Ok(())
}

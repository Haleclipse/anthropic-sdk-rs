// Rust schemars equivalent of TS examples/structured-outputs-zod.ts
//
// TypeScript uses Zod for schema generation + runtime validation.
// Rust equivalent uses schemars for JSON Schema and serde for validation.

use anthropic_sdk::{
    Anthropic, ClientOptions, ContentBlock, MessageContent, MessageCreateParams, MessageParam,
    helpers::schemars::schemars_output_config,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Rust struct that plays the role of the TS schema object.
/// `#[derive(JsonSchema)]` generates the JSON Schema sent to the API.
#[derive(Debug, Deserialize, JsonSchema)]
struct BookRecommendation {
    /// Book title
    title: String,
    /// Author name
    author: String,
    /// Publication year
    year: i32,
    /// Literary genre
    genre: String,
    /// Brief summary
    summary: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct BookList {
    recommendations: Vec<BookRecommendation>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Anthropic::new(ClientOptions::default())?;

    // schemars_output_config::<T>() is the Rust counterpart to TS
    // zodOutputFormat(z.object(...)): it combines schema generation,
    // strict JSON Schema transformation, and output_config wrapping.
    let output_config = schemars_output_config::<BookList>()?;

    let message = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-5-20250929".into(),
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".into(),
                content: MessageContent::Text("Recommend 3 classic science fiction books.".into()),
            }],
            output_config: Some(output_config),
            ..Default::default()
        })
        .await?;

    let text = message
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .ok_or("No text block in response")?;

    // serde_json::from_str() is the runtime validation/parse step.
    let book_list: BookList = serde_json::from_str(text)?;

    println!("Got {} recommendations:\n", book_list.recommendations.len());
    for book in &book_list.recommendations {
        println!("  {} by {} ({})", book.title, book.author, book.year);
        println!("  Genre: {}", book.genre);
        println!("  Summary: {}\n", book.summary);
    }

    Ok(())
}

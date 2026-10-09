// Maps to: TS lib/beta-parser.ts
//
//! Structured output parsing for beta messages.
//!
//! The TypeScript SDK carries a JavaScript parse closure on helper-generated
//! output formats.  Rust exposes the same capability with serde generics: the
//! first beta text content block is deserialized into caller-provided `T`.

use crate::core::error::ApiError;
use crate::resources::beta::messages::{BetaContentBlock, BetaMessage, BetaMessageCreateParams};
use crate::resources::messages::TextCitation;

// Re-export stable parser items for callers that used the previous Rust module
// as a stable-parser alias.
pub use crate::sdk_lib::parser::{ParsedMessage, maybe_parse_message, parse_message};

/// Rust equivalent of TS `BetaParseableMessageCreateParams`.
pub type BetaParseableMessageCreateParams = BetaMessageCreateParams;

/// Rust equivalent of TS `ExtractParsedContentFromBetaParams<Params>`.
pub type ExtractParsedContentFromBetaParams<ParsedT> = ParsedT;

/// Rust marker equivalent of TS `AutoParseableBetaOutputFormat<ParsedT>`.
pub type AutoParseableBetaOutputFormat<ParsedT> =
    crate::sdk_lib::parser::AutoParseableOutputFormat<ParsedT>;

/// Maps to: TS `ParsedBetaContentBlock<ParsedT>`.
#[derive(Debug, Clone)]
pub enum ParsedBetaContentBlock<T> {
    /// A parsed beta text block. TS also exposes a deprecated `parsed` getter;
    /// Rust keeps the canonical `parsed_output` field only.
    Text {
        citations: Option<Vec<TextCitation>>,
        text: String,
        parsed_output: Option<T>,
    },

    /// Any non-text beta content block, unchanged from the API response.
    Other(BetaContentBlock),
}

/// Maps to: TS `ParsedBetaMessage<ParsedT>`.
#[derive(Debug, Clone)]
pub struct ParsedBetaMessage<T> {
    /// The original beta API response message.
    pub message: BetaMessage,

    /// Parsed view of `message.content` with text block `parsed_output` values.
    pub content: Vec<ParsedBetaContentBlock<T>>,

    /// The deserialized value from the first text block, or `None` when the
    /// message contains no text blocks.
    pub parsed_output: Option<T>,
}

/// Maps to: TS `parseBetaMessage()`.
pub fn parse_beta_message<T: serde::de::DeserializeOwned>(
    message: &BetaMessage,
) -> Result<ParsedBetaMessage<T>, ApiError> {
    let mut first_text: Option<&str> = None;
    let mut parsed_content = Vec::with_capacity(message.content.len());

    for block in &message.content {
        match block {
            BetaContentBlock::Text { citations, text } => {
                if first_text.is_none() {
                    first_text = Some(text.as_str());
                }
                let parsed: T = serde_json::from_str(text).map_err(|err| {
                    ApiError::Sdk(format!("Failed to parse structured output: {err}"))
                })?;
                parsed_content.push(ParsedBetaContentBlock::Text {
                    citations: citations.clone(),
                    text: text.clone(),
                    parsed_output: Some(parsed),
                });
            }
            other => parsed_content.push(ParsedBetaContentBlock::Other(other.clone())),
        }
    }

    let parsed_output =
        match first_text {
            Some(raw) => Some(serde_json::from_str(raw).map_err(|err| {
                ApiError::Sdk(format!("Failed to parse structured output: {err}"))
            })?),
            None => None,
        };

    Ok(ParsedBetaMessage {
        message: message.clone(),
        content: parsed_content,
        parsed_output,
    })
}

/// Best-effort beta parser that returns `None` for invalid/missing JSON.
/// Build a parsed beta-message wrapper without attempting to parse text blocks.
///
/// This mirrors the TS beta parser path for params whose output format is
/// absent or not `json_schema`: text blocks receive `parsed_output: null` and
/// no JSON parsing errors are raised.
pub fn parsed_beta_message_without_parsing<T>(message: &BetaMessage) -> ParsedBetaMessage<T> {
    let content = message
        .content
        .iter()
        .map(|block| match block {
            BetaContentBlock::Text { citations, text } => ParsedBetaContentBlock::Text {
                citations: citations.clone(),
                text: text.clone(),
                parsed_output: None,
            },
            other => ParsedBetaContentBlock::Other(other.clone()),
        })
        .collect();

    ParsedBetaMessage {
        message: message.clone(),
        content,
        parsed_output: None,
    }
}

pub fn maybe_parse_beta_message(message: &BetaMessage) -> ParsedBetaMessage<serde_json::Value> {
    let mut first_text_seen = false;
    let mut parsed_output = None;
    let mut parsed_content = Vec::with_capacity(message.content.len());

    for block in &message.content {
        match block {
            BetaContentBlock::Text { citations, text } => {
                let parsed = serde_json::from_str(text).ok();
                if !first_text_seen {
                    parsed_output = parsed.clone();
                    first_text_seen = true;
                }
                parsed_content.push(ParsedBetaContentBlock::Text {
                    citations: citations.clone(),
                    text: text.clone(),
                    parsed_output: parsed,
                });
            }
            other => parsed_content.push(ParsedBetaContentBlock::Other(other.clone())),
        }
    }

    ParsedBetaMessage {
        message: message.clone(),
        content: parsed_content,
        parsed_output,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct TestOutput {
        name: String,
        value: i64,
    }

    fn beta_message(content: Vec<BetaContentBlock>) -> BetaMessage {
        BetaMessage {
            id: "msg_beta".to_owned(),
            content,
            model: "claude-sonnet-4-5-20250929".to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn parse_beta_message_attaches_parsed_output_to_each_text_block_like_ts() {
        let msg = beta_message(vec![
            BetaContentBlock::Text {
                citations: None,
                text: r#"{"name":"first","value":1}"#.to_owned(),
            },
            BetaContentBlock::ToolUse {
                id: "toolu_1".to_owned(),
                input: serde_json::json!({}),
                name: "tool".to_owned(),
                caller: None,
            },
            BetaContentBlock::Text {
                citations: None,
                text: r#"{"name":"second","value":2}"#.to_owned(),
            },
        ]);

        let parsed: ParsedBetaMessage<TestOutput> =
            parse_beta_message(&msg).expect("should parse beta message");
        assert_eq!(parsed.parsed_output.as_ref().unwrap().name, "first");
        assert_eq!(parsed.content.len(), 3);
        match &parsed.content[0] {
            ParsedBetaContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap().name, "first")
            }
            other => panic!("expected text block, got {other:?}"),
        }
        assert!(matches!(
            parsed.content[1],
            ParsedBetaContentBlock::Other(_)
        ));
        match &parsed.content[2] {
            ParsedBetaContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap().name, "second")
            }
            other => panic!("expected text block, got {other:?}"),
        }
    }

    #[test]
    fn parse_beta_message_errors_when_later_text_block_is_invalid_like_ts() {
        let msg = beta_message(vec![
            BetaContentBlock::Text {
                citations: None,
                text: r#"{"name":"first","value":1}"#.to_owned(),
            },
            BetaContentBlock::Text {
                citations: None,
                text: "not json".to_owned(),
            },
        ]);

        let err = parse_beta_message::<TestOutput>(&msg).unwrap_err();
        assert!(
            err.to_string()
                .contains("Failed to parse structured output")
        );
    }

    #[test]
    fn maybe_parse_beta_message_keeps_block_level_none_for_invalid_text() {
        let msg = beta_message(vec![
            BetaContentBlock::Text {
                citations: None,
                text: "not json".to_owned(),
            },
            BetaContentBlock::Text {
                citations: None,
                text: r#"{"ok":true}"#.to_owned(),
            },
        ]);

        let parsed = maybe_parse_beta_message(&msg);
        assert!(parsed.parsed_output.is_none());
        match &parsed.content[0] {
            ParsedBetaContentBlock::Text { parsed_output, .. } => assert!(parsed_output.is_none()),
            other => panic!("expected text block, got {other:?}"),
        }
        match &parsed.content[1] {
            ParsedBetaContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap()["ok"], true)
            }
            other => panic!("expected text block, got {other:?}"),
        }
    }
}

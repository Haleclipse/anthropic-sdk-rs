// Maps to: TS lib/parser.ts
//
// Structured output parsing -- extracts typed data from Message text blocks.
// The TS SDK couples parsing with an `AutoParseableOutputFormat` abstraction
// that carries a `parse` closure. In Rust we lean on serde's `DeserializeOwned`
// instead: callers specify the target type `T` via generics and we deserialize
// the first text block as JSON.

use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::core::error::ApiError;
use crate::resources::messages::{
    ContentBlock, JsonOutputFormat, Message, MessageCreateParams, TextCitation,
};

// ─────────────────────────────────────────────────────────────────────────────
// Root parser type-name parity
// ─────────────────────────────────────────────────────────────────────────────

/// Rust equivalent of TS `ParseableMessageCreateParams`.
///
/// TypeScript widens `MessageCreateParams.output_config.format` so helper
/// output formats can carry a JavaScript `parse` closure. Rust keeps params
/// idiomatic and performs parsing via the generic `T` on [`parse_message`] /
/// `Messages::parse`, so this public name aliases the normal params type.
pub type ParseableMessageCreateParams = MessageCreateParams;

/// Rust marker equivalent of TS `AutoParseableOutputFormat<ParsedT>`.
///
/// It wraps the JSON schema output format and carries the parsed Rust type at
/// compile time. Convert it back into [`JsonOutputFormat`] when assigning to
/// `OutputConfig.format`; `Messages::parse::<T>()` performs serde validation.
#[derive(Debug, Clone)]
pub struct AutoParseableOutputFormat<T> {
    pub format: JsonOutputFormat,
    _marker: PhantomData<fn() -> T>,
}

impl<T> AutoParseableOutputFormat<T> {
    pub fn new(format: JsonOutputFormat) -> Self {
        Self {
            format,
            _marker: PhantomData,
        }
    }

    pub fn as_format(&self) -> &JsonOutputFormat {
        &self.format
    }

    pub fn into_format(self) -> JsonOutputFormat {
        self.format
    }
}

impl<T> From<JsonOutputFormat> for AutoParseableOutputFormat<T> {
    fn from(format: JsonOutputFormat) -> Self {
        Self::new(format)
    }
}

impl<T> From<AutoParseableOutputFormat<T>> for JsonOutputFormat {
    fn from(value: AutoParseableOutputFormat<T>) -> Self {
        value.format
    }
}

impl<T> Serialize for AutoParseableOutputFormat<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.format.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for AutoParseableOutputFormat<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        JsonOutputFormat::deserialize(deserializer).map(Self::new)
    }
}

/// Rust equivalent of TS `ExtractParsedContentFromParams<Params>`.
///
/// TS extracts `ParsedT` from a helper format embedded in the params type. Rust
/// callers provide `ParsedT` explicitly as the generic argument to parse
/// helpers, so this alias preserves the exported name for that parsed type.
pub type ExtractParsedContentFromParams<ParsedT> = ParsedT;

// ─────────────────────────────────────────────────────────────────────────────
// ParsedMessage
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS `ParsedContentBlock<ParsedT>`.
///
/// TS attaches a non-enumerable `parsed_output` to every text block in parsed
/// messages while leaving non-text blocks unchanged. Rust models that as an
/// enum so callers can inspect block-level parsed output without changing the
/// underlying wire [`ContentBlock`] type.
#[derive(Debug, Clone)]
pub enum ParsedContentBlock<T> {
    /// A parsed text block.
    Text {
        citations: Option<Vec<TextCitation>>,
        text: String,
        parsed_output: Option<T>,
    },

    /// Any non-text content block, unchanged from the API response.
    Other(ContentBlock),
}

/// Maps to: TS ParsedMessage<ParsedT>
///
/// Wraps the original `Message` together with parsed block-level content and an
/// optional deserialized value extracted from the first text content block.
#[derive(Debug, Clone)]
pub struct ParsedMessage<T> {
    /// The original API response message.
    pub message: Message,

    /// Parsed view of `message.content` with text block `parsed_output` values.
    pub content: Vec<ParsedContentBlock<T>>,

    /// The deserialized value from the first text block, or `None` when the
    /// message contains no text blocks (e.g. tool-use only responses).
    pub parsed_output: Option<T>,
}

// ─────────────────────────────────────────────────────────────────────────────
// parse_message
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS parseMessage()
///
/// Finds the first `ContentBlock::Text` in `message.content` and attempts to
/// deserialize its text as JSON into `T`.
///
/// - If there are no text blocks, returns `parsed_output: None` (no error).
/// - If deserialization fails, returns `ApiError::Sdk`.
pub fn parse_message<T: serde::de::DeserializeOwned>(
    message: &Message,
) -> Result<ParsedMessage<T>, ApiError> {
    let mut first_text: Option<&str> = None;
    let mut parsed_content = Vec::with_capacity(message.content.len());

    for block in &message.content {
        match block {
            ContentBlock::Text { citations, text } => {
                if first_text.is_none() {
                    first_text = Some(text.as_str());
                }
                let parsed: T = serde_json::from_str(text).map_err(|e| {
                    ApiError::Sdk(format!("Failed to parse structured output: {e}"))
                })?;
                parsed_content.push(ParsedContentBlock::Text {
                    citations: citations.clone(),
                    text: text.clone(),
                    parsed_output: Some(parsed),
                });
            }
            other => parsed_content.push(ParsedContentBlock::Other(other.clone())),
        }
    }

    let parsed_output = match first_text {
        Some(raw) => Some(
            serde_json::from_str(raw)
                .map_err(|e| ApiError::Sdk(format!("Failed to parse structured output: {e}")))?,
        ),
        None => None,
    };

    Ok(ParsedMessage {
        message: message.clone(),
        content: parsed_content,
        parsed_output,
    })
}

/// Build a parsed-message wrapper without attempting to parse text blocks.
///
/// This mirrors the TS parser path for params whose output format is absent or
/// not `json_schema`: text blocks receive `parsed_output: null` and no JSON
/// parsing errors are raised.
pub fn parsed_message_without_parsing<T>(message: &Message) -> ParsedMessage<T> {
    let content = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { citations, text } => ParsedContentBlock::Text {
                citations: citations.clone(),
                text: text.clone(),
                parsed_output: None,
            },
            other => ParsedContentBlock::Other(other.clone()),
        })
        .collect();

    ParsedMessage {
        message: message.clone(),
        content,
        parsed_output: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// maybe_parse_message
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS maybeParseMessage()
///
/// Best-effort counterpart of [`parse_message`]. Attempts to parse the first
/// text block as a `serde_json::Value`; if the text is not valid JSON the
/// `parsed_output` field is simply `None` (no error is raised).
///
/// Use this when you do not know at compile time whether structured output was
/// requested -- it mirrors the TS path where no `output_config.format.parse`
/// function is present.
pub fn maybe_parse_message(message: &Message) -> ParsedMessage<serde_json::Value> {
    let mut first_text_seen = false;
    let mut parsed_output = None;
    let mut parsed_content = Vec::with_capacity(message.content.len());

    for block in &message.content {
        match block {
            ContentBlock::Text { citations, text } => {
                let parsed = serde_json::from_str(text).ok();
                if !first_text_seen {
                    parsed_output = parsed.clone();
                    first_text_seen = true;
                }
                parsed_content.push(ParsedContentBlock::Text {
                    citations: citations.clone(),
                    text: text.clone(),
                    parsed_output: parsed,
                });
            }
            other => parsed_content.push(ParsedContentBlock::Other(other.clone())),
        }
    }

    ParsedMessage {
        message: message.clone(),
        content: parsed_content,
        parsed_output,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::messages::{ContentBlock, Message, StopReason, Usage};

    /// Helper: build a minimal `Message` with the given content blocks.
    fn make_message(content: Vec<ContentBlock>) -> Message {
        Message {
            id: "msg_test".to_owned(),
            request_id: None,
            content,
            model: "claude-sonnet-4-20250514".to_owned(),
            role: "assistant".to_owned(),
            stop_reason: Some(StopReason::EndTurn),
            stop_sequence: None,
            type_name: "message".to_owned(),
            usage: Usage {
                cache_creation: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                inference_geo: None,
                input_tokens: 10,
                output_tokens: 20,
                server_tool_use: None,
                service_tier: None,
            },
        }
    }

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct TestOutput {
        name: String,
        value: i64,
    }

    // -- parse_message ---------------------------------------------------

    #[test]
    fn parse_message_valid_json() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"name":"hello","value":42}"#.to_owned(),
        }]);

        let result: ParsedMessage<TestOutput> = parse_message(&msg).expect("should parse");
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(parsed.name, "hello");
        assert_eq!(parsed.value, 42);
        assert_eq!(result.message.id, "msg_test");
    }

    #[test]
    fn parse_message_no_text_blocks() {
        let msg = make_message(vec![ContentBlock::ToolUse {
            id: "tu_1".to_owned(),
            input: serde_json::json!({}),
            name: "bash".to_owned(),
        }]);

        let result: ParsedMessage<TestOutput> = parse_message(&msg).expect("should succeed");
        assert!(result.parsed_output.is_none());
    }

    #[test]
    fn parse_message_invalid_json_returns_error() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: "not json at all".to_owned(),
        }]);

        let result: Result<ParsedMessage<TestOutput>, ApiError> = parse_message(&msg);
        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::Sdk(m) => assert!(m.contains("Failed to parse structured output")),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn parse_message_picks_first_text_block() {
        let msg = make_message(vec![
            ContentBlock::Thinking {
                signature: String::new(),
                thinking: "hmm".to_owned(),
            },
            ContentBlock::Text {
                citations: None,
                text: r#"{"name":"first","value":1}"#.to_owned(),
            },
            ContentBlock::Text {
                citations: None,
                text: r#"{"name":"second","value":2}"#.to_owned(),
            },
        ]);

        let result: ParsedMessage<TestOutput> = parse_message(&msg).expect("should parse");
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(parsed.name, "first");
    }

    #[test]
    fn parse_message_attaches_parsed_output_to_each_text_block_like_ts() {
        let msg = make_message(vec![
            ContentBlock::Text {
                citations: None,
                text: r#"{"name":"first","value":1}"#.to_owned(),
            },
            ContentBlock::ToolUse {
                id: "tu_1".to_owned(),
                input: serde_json::json!({}),
                name: "tool".to_owned(),
            },
            ContentBlock::Text {
                citations: None,
                text: r#"{"name":"second","value":2}"#.to_owned(),
            },
        ]);

        let result: ParsedMessage<TestOutput> = parse_message(&msg).expect("should parse");
        assert_eq!(result.content.len(), 3);
        match &result.content[0] {
            ParsedContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap().name, "first")
            }
            other => panic!("expected text block, got {other:?}"),
        }
        assert!(matches!(result.content[1], ParsedContentBlock::Other(_)));
        match &result.content[2] {
            ParsedContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap().name, "second")
            }
            other => panic!("expected text block, got {other:?}"),
        }
    }

    #[test]
    fn parse_message_errors_when_later_text_block_is_invalid_like_ts() {
        let msg = make_message(vec![
            ContentBlock::Text {
                citations: None,
                text: r#"{"name":"first","value":1}"#.to_owned(),
            },
            ContentBlock::Text {
                citations: None,
                text: "not json".to_owned(),
            },
        ]);

        let result: Result<ParsedMessage<TestOutput>, ApiError> = parse_message(&msg);
        assert!(matches!(result, Err(ApiError::Sdk(_))));
    }

    // -- maybe_parse_message ---------------------------------------------

    #[test]
    fn maybe_parse_message_valid_json() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"key":"val"}"#.to_owned(),
        }]);

        let result = maybe_parse_message(&msg);
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(parsed["key"], "val");
    }

    #[test]
    fn maybe_parse_message_invalid_json_returns_none() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: "just plain text".to_owned(),
        }]);

        let result = maybe_parse_message(&msg);
        assert!(result.parsed_output.is_none());
    }

    #[test]
    fn maybe_parse_message_no_text_blocks() {
        let msg = make_message(vec![ContentBlock::ToolUse {
            id: "tu_1".to_owned(),
            input: serde_json::json!({}),
            name: "bash".to_owned(),
        }]);

        let result = maybe_parse_message(&msg);
        assert!(result.parsed_output.is_none());
    }

    #[test]
    fn maybe_parse_message_empty_content() {
        let msg = make_message(vec![]);
        let result = maybe_parse_message(&msg);
        assert!(result.parsed_output.is_none());
    }

    // -- TS-parity: parse_message with valid JSON but wrong type --

    #[test]
    fn parse_message_type_mismatch_returns_error() {
        // JSON is syntactically valid but the value field is a string where i64 is expected
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"name":"hello","value":"not a number"}"#.to_owned(),
        }]);

        let result: Result<ParsedMessage<TestOutput>, ApiError> = parse_message(&msg);
        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::Sdk(m) => assert!(m.contains("Failed to parse structured output")),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    // -- TS-parity: parse_message with empty content vec --

    #[test]
    fn parse_message_empty_content() {
        let msg = make_message(vec![]);
        let result: ParsedMessage<TestOutput> = parse_message(&msg).expect("should succeed");
        assert!(result.parsed_output.is_none());
        assert!(result.message.content.is_empty());
    }

    // -- TS-parity: complex schema > union types via enum --

    #[derive(Debug, serde::Deserialize, PartialEq)]
    #[serde(tag = "type")]
    enum PersonOrOrg {
        #[serde(rename = "person")]
        Person { name: String, age: i64 },
        #[serde(rename = "organization")]
        Organization { name: String, employee_count: i64 },
    }

    #[test]
    fn parse_message_union_type() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"type":"person","name":"John","age":30}"#.to_owned(),
        }]);

        let result: ParsedMessage<PersonOrOrg> = parse_message(&msg).expect("should parse");
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(
            parsed,
            PersonOrOrg::Person {
                name: "John".to_owned(),
                age: 30,
            }
        );
    }

    // -- TS-parity: complex schema > optional fields --

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct WithOptional {
        items: Vec<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        has_more: bool,
    }

    #[test]
    fn parse_message_optional_fields() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"items":["a","b"],"description":"test"}"#.to_owned(),
        }]);

        let result: ParsedMessage<WithOptional> = parse_message(&msg).expect("should parse");
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(parsed.items.len(), 2);
        assert_eq!(parsed.description, Some("test".to_owned()));
        assert!(!parsed.has_more);
    }

    // -- TS-parity: complex schema > enums --

    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct WithEnum {
        priority: String,
        status: String,
    }

    #[test]
    fn parse_message_enum_fields() {
        let msg = make_message(vec![ContentBlock::Text {
            citations: None,
            text: r#"{"priority":"high","status":"in_progress"}"#.to_owned(),
        }]);

        let result: ParsedMessage<WithEnum> = parse_message(&msg).expect("should parse");
        let parsed = result.parsed_output.expect("should have parsed output");
        assert_eq!(parsed.priority, "high");
        assert_eq!(parsed.status, "in_progress");
    }
}

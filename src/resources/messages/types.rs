// Maps to: TS resources/messages/messages.ts
//
// Types + Messages resource methods in one file, matching TS SDK structure.
// Contains all serde types for the Messages API (request params, response
// objects, streaming events) plus the `Messages` resource struct with
// `create`, `create_stream`, `stream`, and `count_tokens` methods.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::core::streaming::SseStream;
use crate::internal::request_options::RequestOptions;
use crate::sdk_lib::message_stream::MessageStream;
use crate::sdk_lib::parser::{ParsedMessage, parse_message, parsed_message_without_parsing};

// ==========================================================================
// TS exported-name compatibility aliases
// ==========================================================================

/// TS exports many singleton interfaces that Rust models as enum variants.
/// These aliases keep the TypeScript public names available while preserving
/// idiomatic Rust constructors such as `ImageSource::Base64 { ... }`.
pub type Base64ImageSource = ImageSource;
pub type URLImageSource = ImageSource;
pub type Base64PDFSource = DocumentSource;
pub type PlainTextSource = DocumentSource;
pub type URLPDFSource = DocumentSource;
pub type CitationCharLocation = TextCitation;
pub type CitationPageLocation = TextCitation;
pub type CitationContentBlockLocation = TextCitation;
pub type CitationsSearchResultLocation = TextCitation;
pub type CitationsWebSearchResultLocation = TextCitation;
pub type CitationCharLocationParam = TextCitationParam;
pub type CitationPageLocationParam = TextCitationParam;
pub type CitationContentBlockLocationParam = TextCitationParam;
pub type CitationSearchResultLocationParam = TextCitationParam;
pub type CitationWebSearchResultLocationParam = TextCitationParam;
pub type JSONOutputFormat = JsonOutputFormat;
pub type Model = String;
pub type RawContentBlockDelta = ContentBlockDelta;
pub type TextDelta = ContentBlockDelta;
pub type InputJSONDelta = ContentBlockDelta;
pub type CitationsDelta = CitationsDeltaData;
pub type ThinkingDelta = ContentBlockDelta;
pub type SignatureDelta = ContentBlockDelta;
pub type RawMessageStreamEvent = MessageStreamEvent;
pub type RawMessageStartEvent = MessageStreamEvent;
pub type RawMessageDeltaEvent = MessageStreamEvent;
pub type RawMessageStopEvent = MessageStreamEvent;
pub type RawContentBlockStartEvent = MessageStreamEvent;
pub type RawContentBlockDeltaEvent = MessageStreamEvent;
pub type RawContentBlockStopEvent = MessageStreamEvent;
pub type MessageStartEvent = MessageStreamEvent;
pub type MessageDeltaEvent = MessageStreamEvent;
pub type MessageStopEvent = MessageStreamEvent;
pub type ContentBlockStartEvent = MessageStreamEvent;
pub type ContentBlockDeltaEvent = MessageStreamEvent;
pub type ContentBlockStopEvent = MessageStreamEvent;
pub type ThinkingConfigParam = ThinkingConfig;
pub type ThinkingConfigEnabled = ThinkingConfig;
pub type ThinkingConfigDisabled = ThinkingConfig;
pub type ThinkingConfigAdaptive = ThinkingConfig;
pub type ToolChoiceAuto = ToolChoice;
pub type ToolChoiceAny = ToolChoice;
pub type ToolChoiceTool = ToolChoice;
pub type ToolChoiceNone = ToolChoice;
pub type ToolTextEditor20250124 = ToolTextEditor20250124Typed;
pub type ToolTextEditor20250429 = ToolTextEditor20250429Typed;
pub type MessageCreateParamsBase = MessageCreateParams;
pub type MessageCreateParamsNonStreaming = MessageCreateParams;
pub type MessageCreateParamsStreaming = MessageCreateParams;
pub type MessageStreamParams = MessageCreateParams;

// ==========================================================================
// Types
// ==========================================================================

// --------------------------------------------------------------------------
// Stop Reason
// --------------------------------------------------------------------------

/// Maps to: TS StopReason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    PauseTurn,
    Refusal,
}

// --------------------------------------------------------------------------
// Usage & related
// --------------------------------------------------------------------------

/// Maps to: TS CacheCreation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheCreation {
    pub ephemeral_1h_input_tokens: i64,
    pub ephemeral_5m_input_tokens: i64,
}

/// Maps to: TS ServerToolUsage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUsage {
    pub web_search_requests: i64,
}

/// Maps to: TS Usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<CacheCreation>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,

    pub input_tokens: i64,
    pub output_tokens: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUsage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
}

/// Maps to: TS ServiceTier
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    Standard,
    Priority,
    Batch,
}

// --------------------------------------------------------------------------
// Citation types
// --------------------------------------------------------------------------

/// Maps to: TS TextCitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TextCitation {
    /// Maps to: TS CitationCharLocation
    #[serde(rename = "char_location")]
    CharLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_char_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        start_char_index: i64,
    },

    /// Maps to: TS CitationPageLocation
    #[serde(rename = "page_location")]
    PageLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_page_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        start_page_number: i64,
    },

    /// Maps to: TS CitationContentBlockLocation
    #[serde(rename = "content_block_location")]
    ContentBlockLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_block_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        start_block_index: i64,
    },

    /// Maps to: TS CitationsWebSearchResultLocation
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        url: String,
    },

    /// Maps to: TS CitationsSearchResultLocation
    #[serde(rename = "search_result_location")]
    SearchResultLocation {
        cited_text: String,
        end_block_index: i64,
        search_result_index: i64,
        source: String,
        start_block_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
}

// --------------------------------------------------------------------------
// Citation param types (request-side, without file_id)
// --------------------------------------------------------------------------

/// Maps to: TS TextCitationParam
///
/// Request-side citation types used in `TextBlockParam.citations`.
/// Unlike the response-side `TextCitation`, these do not include `file_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TextCitationParam {
    /// Maps to: TS CitationCharLocationParam
    #[serde(rename = "char_location")]
    CharLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_char_index: i64,
        start_char_index: i64,
    },

    /// Maps to: TS CitationPageLocationParam
    #[serde(rename = "page_location")]
    PageLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_page_number: i64,
        start_page_number: i64,
    },

    /// Maps to: TS CitationContentBlockLocationParam
    #[serde(rename = "content_block_location")]
    ContentBlockLocation {
        cited_text: String,
        document_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        document_title: Option<String>,
        end_block_index: i64,
        start_block_index: i64,
    },

    /// Maps to: TS CitationsWebSearchResultLocationParam
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        url: String,
    },

    /// Maps to: TS CitationsSearchResultLocationParam
    #[serde(rename = "search_result_location")]
    SearchResultLocation {
        cited_text: String,
        end_block_index: i64,
        search_result_index: i64,
        source: String,
        start_block_index: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
}

// --------------------------------------------------------------------------
// Web Search Result types
// --------------------------------------------------------------------------

/// Maps to: TS WebSearchResultBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResultBlock {
    pub encrypted_content: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,

    pub title: String,

    #[serde(rename = "type")]
    pub type_name: String,

    pub url: String,
}

/// Maps to: TS WebSearchResultBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResultBlockParam {
    pub encrypted_content: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

impl Default for WebSearchResultBlock {
    fn default() -> Self {
        Self {
            encrypted_content: String::new(),
            page_age: None,
            title: String::new(),
            type_name: "web_search_result".to_owned(),
            url: String::new(),
        }
    }
}

/// Maps to: TS WebSearchToolResultError
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolResultError {
    pub error_code: WebSearchErrorCode,

    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS WebSearchToolRequestError
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolRequestError {
    pub error_code: WebSearchErrorCode,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS WebSearchErrorCode
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchErrorCode {
    InvalidToolInput,
    Unavailable,
    MaxUsesExceeded,
    TooManyRequests,
    QueryTooLong,
    RequestTooLarge,
}

impl WebSearchErrorCode {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidToolInput => "invalid_tool_input",
            Self::Unavailable => "unavailable",
            Self::MaxUsesExceeded => "max_uses_exceeded",
            Self::TooManyRequests => "too_many_requests",
            Self::QueryTooLong => "query_too_long",
            Self::RequestTooLarge => "request_too_large",
        }
    }
}

impl std::fmt::Display for WebSearchErrorCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Maps to: TS WebSearchToolResultBlockContent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSearchToolResultBlockContent {
    Error(WebSearchToolResultError),
    Results(Vec<WebSearchResultBlock>),
}

/// Maps to: TS WebSearchToolResultBlockParamContent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSearchToolResultBlockParamContent {
    Error(WebSearchToolRequestError),
    Results(Vec<WebSearchResultBlockParam>),
}

// --------------------------------------------------------------------------
// Content blocks (response)
// --------------------------------------------------------------------------

/// Maps to: TS ContentBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Maps to: TS TextBlock
    #[serde(rename = "text")]
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<TextCitation>>,
        text: String,
    },

    /// Maps to: TS ThinkingBlock
    ///
    /// `signature` defaults to `""` so SSE `content_block_start` events that omit
    /// the field (filled later by `signature_delta`) match the TS SDK runtime,
    /// which only `JSON.parse`s and does not schema-validate.
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        signature: String,
        thinking: String,
    },

    /// Maps to: TS RedactedThinkingBlock
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },

    /// Maps to: TS ToolUseBlock
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },

    /// Maps to: TS ServerToolUseBlock
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },

    /// Maps to: TS WebSearchToolResultBlock
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        content: WebSearchToolResultBlockContent,
        tool_use_id: String,
    },
}

/// Maps to: TS TextBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<TextCitation>>,
    pub text: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for TextBlock {
    fn default() -> Self {
        Self {
            citations: None,
            text: String::new(),
            type_name: "text".to_owned(),
        }
    }
}

/// Maps to: TS ThinkingBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlock {
    /// Defaults to `""` for streaming start events that omit signature.
    #[serde(default)]
    pub signature: String,
    pub thinking: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for ThinkingBlock {
    fn default() -> Self {
        Self {
            signature: String::new(),
            thinking: String::new(),
            type_name: "thinking".to_owned(),
        }
    }
}

/// Maps to: TS RedactedThinkingBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedThinkingBlock {
    pub data: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for RedactedThinkingBlock {
    fn default() -> Self {
        Self {
            data: String::new(),
            type_name: "redacted_thinking".to_owned(),
        }
    }
}

/// Maps to: TS ToolUseBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseBlock {
    pub id: String,
    pub input: serde_json::Value,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for ToolUseBlock {
    fn default() -> Self {
        Self {
            id: String::new(),
            input: serde_json::Value::Object(serde_json::Map::new()),
            name: String::new(),
            type_name: "tool_use".to_owned(),
        }
    }
}

/// Maps to: TS ServerToolUseBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUseBlock {
    pub id: String,
    pub input: serde_json::Value,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for ServerToolUseBlock {
    fn default() -> Self {
        Self {
            id: String::new(),
            input: serde_json::Value::Object(serde_json::Map::new()),
            name: String::new(),
            type_name: "server_tool_use".to_owned(),
        }
    }
}

/// Maps to: TS WebSearchToolResultBlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolResultBlock {
    pub content: WebSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for WebSearchToolResultBlock {
    fn default() -> Self {
        Self {
            content: WebSearchToolResultBlockContent::Results(vec![]),
            tool_use_id: String::new(),
            type_name: "web_search_tool_result".to_owned(),
        }
    }
}

// --------------------------------------------------------------------------
// Message (response)
// --------------------------------------------------------------------------

/// Maps to: TS Message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(rename = "_request_id", default, skip_serializing)]
    pub request_id: Option<String>,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub role: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,

    #[serde(rename = "type")]
    pub type_name: String,

    pub usage: Usage,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            id: String::new(),
            request_id: None,
            content: vec![],
            model: String::new(),
            role: "assistant".to_owned(),
            stop_reason: None,
            stop_sequence: None,
            type_name: "message".to_owned(),
            usage: Usage {
                cache_creation: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                inference_geo: None,
                input_tokens: 0,
                output_tokens: 0,
                server_tool_use: None,
                service_tier: None,
            },
        }
    }
}

// --------------------------------------------------------------------------
// Cache control
// --------------------------------------------------------------------------

/// Maps to: TS CacheControlEphemeral
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControlEphemeral {
    #[serde(rename = "type")]
    pub type_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

impl Default for CacheControlEphemeral {
    fn default() -> Self {
        Self {
            type_name: "ephemeral".to_owned(),
            ttl: None,
        }
    }
}

// --------------------------------------------------------------------------
// Request content block params
// --------------------------------------------------------------------------

/// Maps to: TS TextBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlockParam {
    pub text: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<TextCitationParam>>,

    /// The type discriminator. Always `"text"`. Required for standalone use
    /// (e.g. in `SystemPrompt::Blocks` or `SearchResultBlockParam.content`)
    /// where the enclosing `ContentBlockParam` enum tag is absent.
    #[serde(
        rename = "type",
        default = "TextBlockParam::default_type_name",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_name: Option<String>,
}

impl TextBlockParam {
    fn default_type_name() -> Option<String> {
        None
    }
}

/// Maps to: TS CitationsConfigParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationsConfigParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Maps to: TS ImageSource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageSource {
    /// Maps to: TS Base64ImageSource
    #[serde(rename = "base64")]
    Base64 { data: String, media_type: String },

    /// Maps to: TS UrlImageSource
    #[serde(rename = "url")]
    Url { url: String },
}

/// Maps to: TS ImageBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBlockParam {
    pub source: ImageSource,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS ContentBlockSource (used in ContentSource.content)
///
/// Represents a content block within a document source. Can be either a
/// text block or an image block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockSourceContent {
    /// A text content block.
    #[serde(rename = "text")]
    Text(TextBlockParam),

    /// An image content block.
    #[serde(rename = "image")]
    Image(ImageBlockParam),
}

/// Maps to: TS ContentSource.content (String | Array<ContentBlockSourceContent>)
///
/// The data for a content-based document source. Can be either a plain string
/// or a list of typed content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlockSourceData {
    /// Plain string content.
    Text(String),
    /// Array of typed content blocks.
    Blocks(Vec<ContentBlockSourceContent>),
}

/// Maps to: TS ContentBlockSource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlockSource {
    pub content: ContentBlockSourceData,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS DocumentSource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DocumentSource {
    /// Maps to: TS Base64PdfSource
    #[serde(rename = "base64")]
    Base64Pdf { data: String, media_type: String },

    /// Maps to: TS PlainTextSource
    #[serde(rename = "text")]
    PlainText { data: String, media_type: String },

    /// Maps to: TS ContentSource
    #[serde(rename = "content")]
    Content { content: ContentBlockSourceData },

    /// Maps to: TS UrlPdfSource
    #[serde(rename = "url")]
    UrlPdf { url: String },
}

/// Maps to: TS DocumentBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBlockParam {
    pub source: DocumentSource,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Maps to: TS ThinkingBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingBlockParam {
    /// Defaults to `""` when absent (TS runtime accepts missing then fills later).
    #[serde(default)]
    pub signature: String,
    pub thinking: String,
}

/// Maps to: TS RedactedThinkingBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedThinkingBlockParam {
    pub data: String,
}

/// Maps to: TS ToolUseBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseBlockParam {
    pub id: String,
    pub input: serde_json::Value,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS ToolResultContent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<serde_json::Value>),
}

/// Maps to: TS ToolResultBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlockParam {
    pub tool_use_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ToolResultContent>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Maps to: TS ServerToolUseBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolUseBlockParam {
    pub id: String,
    pub input: serde_json::Value,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS WebSearchToolResultBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchToolResultBlockParam {
    pub content: WebSearchToolResultBlockParamContent,
    pub tool_use_id: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS SearchResultBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultBlockParam {
    pub content: Vec<TextBlockParam>,
    pub source: String,
    pub title: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,
}

/// Maps to: TS ContentBlockParam
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockParam {
    /// Maps to: TS TextBlockParam
    #[serde(rename = "text")]
    Text(TextBlockParam),

    /// Maps to: TS ImageBlockParam
    #[serde(rename = "image")]
    Image(ImageBlockParam),

    /// Maps to: TS DocumentBlockParam
    #[serde(rename = "document")]
    Document(DocumentBlockParam),

    /// Maps to: TS ThinkingBlockParam
    #[serde(rename = "thinking")]
    Thinking(ThinkingBlockParam),

    /// Maps to: TS RedactedThinkingBlockParam
    #[serde(rename = "redacted_thinking")]
    RedactedThinking(RedactedThinkingBlockParam),

    /// Maps to: TS ToolUseBlockParam
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseBlockParam),

    /// Maps to: TS ToolResultBlockParam
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultBlockParam),

    /// Maps to: TS ServerToolUseBlockParam
    #[serde(rename = "server_tool_use")]
    ServerToolUse(ServerToolUseBlockParam),

    /// Maps to: TS WebSearchToolResultBlockParam
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult(WebSearchToolResultBlockParam),

    /// Maps to: TS SearchResultBlockParam
    #[serde(rename = "search_result")]
    SearchResult(SearchResultBlockParam),
}

// --------------------------------------------------------------------------
// MessageParam & MessageContent
// --------------------------------------------------------------------------

/// Maps to: TS MessageContent (string | Array<ContentBlockParam>)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlockParam>),
}

/// Maps to: TS MessageParam
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageParam {
    pub content: MessageContent,
    pub role: String,
}

/// Maps to: TS SystemPrompt (string | Array<TextBlockParam>)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<TextBlockParam>),
}

// --------------------------------------------------------------------------
// Tool types
// --------------------------------------------------------------------------

/// Maps to: TS Tool.InputSchema.
pub type ToolInputSchema = serde_json::Value;

/// Back-compatibility alias for the TS nested name `Tool.InputSchema` where
/// Rust cannot define an associated interface namespace for a struct.
pub type InputSchema = ToolInputSchema;

/// Maps to: TS Tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub input_schema: ToolInputSchema,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Maps to: TS Tool.eager_input_streaming
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,

    /// Maps to: TS Tool.strict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

/// Maps to: TS ToolChoice
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolChoice {
    /// Maps to: TS ToolChoiceAuto
    #[serde(rename = "auto")]
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },

    /// Maps to: TS ToolChoiceAny
    #[serde(rename = "any")]
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },

    /// Maps to: TS ToolChoiceTool
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },

    /// Maps to: TS ToolChoiceNone
    #[serde(rename = "none")]
    None,
}

/// Maps to: TS ToolBash20250124
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolBash20250124 {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Maps to: TS ToolTextEditor20250124 / ToolTextEditor20250429
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTextEditor {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Maps to: TS ToolTextEditor20250728
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTextEditor20250728 {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<i64>,
}

/// Maps to: TS WebSearchUserLocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchUserLocation {
    #[serde(rename = "type")]
    pub type_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Rust alias for TS `WebSearchTool20250305.UserLocation`.
pub type WebSearchTool20250305UserLocation = WebSearchUserLocation;

/// Rust best-effort alias for the TS nested name `UserLocation`.
pub type UserLocation = WebSearchUserLocation;

/// Maps to: TS WebSearchTool20250305
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchTool20250305 {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<WebSearchUserLocation>,
}

/// Maps to: TS ToolUnion.
///
/// The `WithHelper` wrapper is Rust's equivalent of the TS SDK's hidden
/// `SDK_HELPER_SYMBOL`: it does not serialize into the request body, but the
/// Messages resource uses it to populate `x-stainless-helper`.
#[derive(Debug, Clone)]
pub enum ToolUnion {
    /// Maps to: TS Tool (custom)
    Custom(Tool),
    /// Maps to: TS ToolBash20250124
    Bash(ToolBash20250124Typed),
    /// Maps to: TS ToolTextEditor20250124
    TextEditor20250124(ToolTextEditor20250124Typed),
    /// Maps to: TS ToolTextEditor20250429
    TextEditor20250429(ToolTextEditor20250429Typed),
    /// Maps to: TS ToolTextEditor20250728
    TextEditor20250728(ToolTextEditor20250728Typed),
    /// Maps to: TS WebSearchTool20250305
    WebSearch(WebSearchTool20250305Typed),
    /// Lossless extension point for beta/server tools that the Rust SDK has not
    /// typed yet, matching the TypeScript SDK's structurally-typed ToolUnion.
    Raw(serde_json::Value),
    /// Helper metadata wrapper; serializes exactly like the wrapped tool.
    WithHelper {
        tool: Box<ToolUnion>,
        stainless_helper: String,
    },
}

impl ToolUnion {
    /// Attach a stainless-helper marker without changing the wire payload.
    pub fn with_stainless_helper(self, helper: impl Into<String>) -> Self {
        Self::WithHelper {
            tool: Box::new(self),
            stainless_helper: helper.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum ToolUnionWire {
    Custom(Tool),
    Bash(ToolBash20250124Typed),
    TextEditor20250124(ToolTextEditor20250124Typed),
    TextEditor20250429(ToolTextEditor20250429Typed),
    TextEditor20250728(ToolTextEditor20250728Typed),
    WebSearch(WebSearchTool20250305Typed),
    Raw(serde_json::Value),
}

impl From<ToolUnionWire> for ToolUnion {
    fn from(value: ToolUnionWire) -> Self {
        match value {
            ToolUnionWire::Custom(tool) => ToolUnion::Custom(tool),
            ToolUnionWire::Bash(tool) => ToolUnion::Bash(tool),
            ToolUnionWire::TextEditor20250124(tool) => ToolUnion::TextEditor20250124(tool),
            ToolUnionWire::TextEditor20250429(tool) => ToolUnion::TextEditor20250429(tool),
            ToolUnionWire::TextEditor20250728(tool) => ToolUnion::TextEditor20250728(tool),
            ToolUnionWire::WebSearch(tool) => ToolUnion::WebSearch(tool),
            ToolUnionWire::Raw(tool) => ToolUnion::Raw(tool),
        }
    }
}

impl Serialize for ToolUnion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ToolUnion::Custom(tool) => tool.serialize(serializer),
            ToolUnion::Bash(tool) => tool.serialize(serializer),
            ToolUnion::TextEditor20250124(tool) => tool.serialize(serializer),
            ToolUnion::TextEditor20250429(tool) => tool.serialize(serializer),
            ToolUnion::TextEditor20250728(tool) => tool.serialize(serializer),
            ToolUnion::WebSearch(tool) => tool.serialize(serializer),
            ToolUnion::Raw(tool) => tool.serialize(serializer),
            ToolUnion::WithHelper { tool, .. } => tool.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ToolUnion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        ToolUnionWire::deserialize(deserializer).map(Into::into)
    }
}

/// Maps to: TS ToolBash20250124 (with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolBash20250124Typed {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for ToolBash20250124Typed {
    fn default() -> Self {
        Self {
            type_name: "bash_20250124".to_owned(),
            name: String::new(),
            cache_control: None,
            strict: None,
        }
    }
}

/// Maps to: TS ToolTextEditor20250124 (with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTextEditor20250124Typed {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for ToolTextEditor20250124Typed {
    fn default() -> Self {
        Self {
            type_name: "text_editor_20250124".to_owned(),
            name: String::new(),
            cache_control: None,
            strict: None,
        }
    }
}

/// Maps to: TS ToolTextEditor20250429 (with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTextEditor20250429Typed {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for ToolTextEditor20250429Typed {
    fn default() -> Self {
        Self {
            type_name: "text_editor_20250429".to_owned(),
            name: String::new(),
            cache_control: None,
            strict: None,
        }
    }
}

/// Maps to: TS ToolTextEditor20250728 (with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTextEditor20250728Typed {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<i64>,
}

impl Default for ToolTextEditor20250728Typed {
    fn default() -> Self {
        Self {
            type_name: "text_editor_20250728".to_owned(),
            name: String::new(),
            cache_control: None,
            strict: None,
            max_characters: None,
        }
    }
}

/// Maps to: TS WebSearchTool20250305 (with type discriminator)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchTool20250305Typed {
    #[serde(rename = "type")]
    pub type_name: String,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<WebSearchUserLocation>,
}

impl Default for WebSearchTool20250305Typed {
    fn default() -> Self {
        Self {
            type_name: "web_search_20250305".to_owned(),
            name: String::new(),
            allowed_domains: None,
            blocked_domains: None,
            cache_control: None,
            max_uses: None,
            strict: None,
            user_location: None,
        }
    }
}

// --------------------------------------------------------------------------
// Thinking config
// --------------------------------------------------------------------------

/// Maps to: TS ThinkingConfigParam
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ThinkingConfig {
    /// Maps to: TS ThinkingConfigEnabled
    #[serde(rename = "enabled")]
    Enabled { budget_tokens: i64 },

    /// Maps to: TS ThinkingConfigDisabled
    #[serde(rename = "disabled")]
    Disabled,

    /// Maps to: TS ThinkingConfigAdaptive
    #[serde(rename = "adaptive")]
    Adaptive,
}

// --------------------------------------------------------------------------
// Output config & metadata
// --------------------------------------------------------------------------

/// Maps to: TS JsonOutputFormat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonOutputFormat {
    pub schema: serde_json::Value,

    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for JsonOutputFormat {
    fn default() -> Self {
        Self {
            schema: serde_json::Value::Object(serde_json::Map::new()),
            type_name: "json_schema".to_owned(),
        }
    }
}

/// Maps to: TS Effort
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Max,
}

/// Maps to: TS OutputConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsonOutputFormat>,
}

/// Maps to: TS Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// --------------------------------------------------------------------------
// MessageCreateParams (request)
// --------------------------------------------------------------------------

/// Maps to: TS MessageCreateParams
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageCreateParams {
    pub max_tokens: i64,
    pub messages: Vec<MessageParam>,
    pub model: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolUnion>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

// --------------------------------------------------------------------------
// Streaming event types
// --------------------------------------------------------------------------

/// Maps to: TS RawMessageDeltaEvent.Delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// Rust alias for TS `RawMessageDeltaEvent.Delta`.
pub type RawMessageDeltaEventDelta = MessageDelta;

/// Rust best-effort alias for the TS nested name `Delta`.
pub type Delta = MessageDelta;

/// Maps to: TS MessageDeltaUsage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeltaUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,

    pub output_tokens: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<ServerToolUsage>,
}

/// Maps to: TS CitationsDelta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationsDeltaData {
    #[serde(rename = "type")]
    pub type_name: String,

    pub citation: TextCitation,
}

/// Maps to: TS RawContentBlockDelta
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlockDelta {
    /// Maps to: TS TextDelta
    #[serde(rename = "text_delta")]
    TextDelta { text: String },

    /// Maps to: TS InputJsonDelta
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },

    /// Maps to: TS CitationsDelta
    #[serde(rename = "citations_delta")]
    CitationsDelta { citation: TextCitation },

    /// Maps to: TS ThinkingDelta
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },

    /// Maps to: TS SignatureDelta
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
}

/// Maps to: TS RawMessageStreamEvent (MessageStreamEvent is an alias)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageStreamEvent {
    /// Maps to: TS RawMessageStartEvent
    #[serde(rename = "message_start")]
    MessageStart { message: Message },

    /// Maps to: TS RawMessageDeltaEvent
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDelta,
        usage: MessageDeltaUsage,
    },

    /// Maps to: TS RawMessageStopEvent
    #[serde(rename = "message_stop")]
    MessageStop,

    /// Maps to: TS RawContentBlockStartEvent
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        content_block: ContentBlock,
        index: usize,
    },

    /// Maps to: TS RawContentBlockDeltaEvent
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        delta: ContentBlockDelta,
        index: usize,
    },

    /// Maps to: TS RawContentBlockStopEvent
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },

    /// Maps to: TS PingEvent
    #[serde(rename = "ping")]
    Ping,
}

// --------------------------------------------------------------------------
// Token counting
// --------------------------------------------------------------------------

/// Maps to: TS MessageTokensCount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTokensCount {
    pub input_tokens: i64,
}

/// Maps to: TS MessageCountTokensTool
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageCountTokensTool {
    Custom(Tool),
    Bash(ToolBash20250124Typed),
    TextEditor20250124(ToolTextEditor20250124Typed),
    TextEditor20250429(ToolTextEditor20250429Typed),
    TextEditor20250728(ToolTextEditor20250728Typed),
    WebSearch(WebSearchTool20250305Typed),
}

/// Maps to: TS MessageCountTokensParams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCountTokensParams {
    pub messages: Vec<MessageParam>,
    pub model: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<MessageCountTokensTool>>,
}

// ==========================================================================
// Messages resource
// ==========================================================================

fn has_json_schema_output_format(params: &MessageCreateParams) -> bool {
    params
        .output_config
        .as_ref()
        .and_then(|config| config.format.as_ref())
        .is_some_and(|format| format.type_name == "json_schema")
}

fn deprecated_model_eol(model: &str, beta: bool) -> Option<&'static str> {
    match model {
        "claude-1.3"
        | "claude-1.3-100k"
        | "claude-instant-1.1"
        | "claude-instant-1.1-100k"
        | "claude-instant-1.2" => Some("November 6th, 2024"),
        "claude-3-sonnet-20240229" | "claude-2.1" | "claude-2.0" => Some("July 21st, 2025"),
        "claude-3-opus-20240229" => Some("January 5th, 2026"),
        "claude-3-7-sonnet-latest" | "claude-3-7-sonnet-20250219" => Some("February 19th, 2026"),
        // Stable TS messages.ts includes these; beta messages.ts v0.74.0 does not.
        "claude-3-5-haiku-latest" | "claude-3-5-haiku-20241022" if !beta => {
            Some("February 19th, 2026")
        }
        _ => None,
    }
}

pub(crate) fn message_create_warning_messages(
    model: &str,
    thinking: Option<&ThinkingConfig>,
    beta: bool,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if let Some(eol) = deprecated_model_eol(model, beta) {
        warnings.push(format!(
            "The model '{model}' is deprecated and will reach end-of-life on {eol}\nPlease migrate to a newer model. Visit https://docs.anthropic.com/en/docs/resources/model-deprecations for more information."
        ));
    }

    if model == "claude-opus-4-6" && matches!(thinking, Some(ThinkingConfig::Enabled { .. })) {
        warnings.push(format!(
            "Using Claude with {model} and 'thinking.type=enabled' is deprecated. Use 'thinking.type=adaptive' instead which results in better model performance in our testing: https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking"
        ));
    }

    warnings
}

pub(crate) fn emit_message_create_warnings(
    model: &str,
    thinking: Option<&ThinkingConfig>,
    beta: bool,
) {
    for warning in message_create_warning_messages(model, thinking, beta) {
        tracing::warn!("{warning}");
    }
}

fn message_helper_headers(tools: Option<&[ToolUnion]>) -> Option<HashMap<String, Option<String>>> {
    let helpers = collect_tool_stainless_helpers(tools);
    if helpers.is_empty() {
        return None;
    }

    let mut headers = HashMap::new();
    headers.insert("x-stainless-helper".to_owned(), Some(helpers.join(", ")));
    Some(headers)
}

fn collect_tool_stainless_helpers(tools: Option<&[ToolUnion]>) -> Vec<String> {
    let mut helpers = Vec::new();
    if let Some(tools) = tools {
        for tool in tools {
            if let Some(helper) = tool_stainless_helper(tool) {
                if !helpers.iter().any(|existing| existing == helper) {
                    helpers.push(helper.to_owned());
                }
            }
        }
    }
    helpers
}

fn tool_stainless_helper(tool: &ToolUnion) -> Option<&str> {
    match tool {
        ToolUnion::WithHelper {
            stainless_helper, ..
        } => Some(stainless_helper.as_str()),
        _ => None,
    }
}

/// Maps to: TS Messages class -- resource-based accessor for the Messages API.
///
/// Obtain an instance via [`Anthropic::messages`]:
///
/// ```ignore
/// let client = Anthropic::new(ClientOptions::default())?;
/// let msg = client.messages().create(&params).await?;
/// ```
pub struct Messages<'a> {
    client: &'a Anthropic,
}

impl<'a> Messages<'a> {
    /// Create a new `Messages` resource bound to the given client.
    ///
    /// Callers should use [`Anthropic::messages`] rather than constructing
    /// this directly.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Access the message Batches sub-resource.
    ///
    /// Maps to: TS `Messages.batches`.
    pub fn batches(&self) -> super::batches::Batches<'a> {
        super::batches::Batches::new(self.client)
    }

    /// Maps to: TS Messages.create() -- POST /v1/messages
    ///
    /// Sends a non-streaming message creation request and returns the full
    /// [`Message`] response once complete.
    ///
    /// The `stream` field on `params` is forced to `false` (or omitted)
    /// regardless of its incoming value.
    pub async fn create(&self, params: &MessageCreateParams) -> Result<Message, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Message creation with per-request options.
    pub async fn create_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<Message, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), false);
        self.client
            .validate_nonstreaming_timeout(&params.model, params.max_tokens, options)?;

        // Build a body with stream explicitly set to false.
        let mut body = serde_json::to_value(params)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize MessageCreateParams: {e}")))?;
        body.as_object_mut()
            .ok_or_else(|| ApiError::Sdk("MessageCreateParams did not serialize to object".into()))?
            .insert("stream".into(), serde_json::Value::Bool(false));

        let extra_headers = message_helper_headers(params.tools.as_deref());

        self.client
            .post_with_options("/v1/messages", &body, extra_headers.as_ref(), options)
            .await
    }

    /// Rust equivalent of TS `client.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<Message>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Message creation returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Message>, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), false);
        self.client
            .validate_nonstreaming_timeout(&params.model, params.max_tokens, options)?;

        let mut body = serde_json::to_value(params)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize MessageCreateParams: {e}")))?;
        body.as_object_mut()
            .ok_or_else(|| ApiError::Sdk("MessageCreateParams did not serialize to object".into()))?
            .insert("stream".into(), serde_json::Value::Bool(false));

        let extra_headers = message_helper_headers(params.tools.as_deref());

        self.client
            .post_with_response("/v1/messages", &body, extra_headers.as_ref(), options)
            .await
    }

    /// Maps to: TS Messages.parse() -- create then parse structured output.
    ///
    /// Rust uses serde generics instead of the TS Zod parse closure: callers
    /// provide the target type `T`, and the first text content block is
    /// deserialized as JSON.
    pub async fn parse<T>(&self, params: &MessageCreateParams) -> Result<ParsedMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_options(params, None).await
    }

    /// Structured-output parse with per-request options.
    pub async fn parse_with_options<T>(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ParsedMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        Ok(self
            .parse_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Structured-output parse returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .create_with_response_and_options(params, options)
            .await?;
        let parsed = if has_json_schema_output_format(params) {
            parse_message(&response.data)?
        } else {
            parsed_message_without_parsing(&response.data)
        };
        Ok(ApiResponse {
            data: parsed,
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to: TS Messages.create() with stream:true -- POST /v1/messages
    ///
    /// Sends a streaming message creation request and returns a raw
    /// [`SseStream`] that yields individual [`MessageStreamEvent`]s as they
    /// arrive over the wire.
    ///
    /// For a higher-level interface that accumulates deltas into a final
    /// [`Message`], see [`Messages::stream`].
    pub async fn create_stream(
        &self,
        params: &MessageCreateParams,
    ) -> Result<SseStream<MessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    /// Streaming message creation with per-request options.
    pub async fn create_stream_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SseStream<MessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.create({ stream: true }).withResponse()`.
    ///
    /// The raw response body remains owned by the returned SSE stream, so
    /// `response.body` is intentionally empty and only metadata/headers are
    /// captured.
    pub async fn create_stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Streaming creation with response metadata and per-request options.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), false);
        // Build a body with stream explicitly set to true.
        let mut body = serde_json::to_value(params)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize MessageCreateParams: {e}")))?;
        body.as_object_mut()
            .ok_or_else(|| ApiError::Sdk("MessageCreateParams did not serialize to object".into()))?
            .insert("stream".into(), serde_json::Value::Bool(true));

        let extra_headers = message_helper_headers(params.tools.as_deref());

        let response = self
            .client
            .post_stream_with_options("/v1/messages", &body, extra_headers.as_ref(), options)
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: SseStream::new(response),
            response: raw,
            request_id,
        })
    }

    /// Maps to: TS Messages.stream() -- high-level streaming helper
    ///
    /// Returns a [`MessageStream`] that internally calls [`Messages::create_stream`],
    /// accumulates deltas, and exposes both the event stream and a
    /// [`MessageStream::final_message`] accessor for the fully-assembled
    /// [`Message`].
    pub async fn stream(&self, params: &MessageCreateParams) -> Result<MessageStream, ApiError> {
        self.stream_with_options(params, None).await
    }

    /// High-level streaming helper with per-request options.
    pub async fn stream_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageStream, ApiError> {
        Ok(self
            .stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `MessageStream.withResponse()` for streams
    /// created from the resource helper.
    pub async fn stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// High-level streaming helper with response metadata and per-request options.
    pub async fn stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        let mut helper_options = options.cloned().unwrap_or_default();
        helper_options
            .headers
            .get_or_insert_with(HashMap::new)
            .insert(
                "X-Stainless-Helper-Method".to_owned(),
                Some("stream".to_owned()),
            );

        let response = self
            .create_stream_with_response_and_options(params, Some(&helper_options))
            .await?;
        Ok(ApiResponse {
            data: MessageStream::new(Box::pin(response.data)),
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to: TS Messages.countTokens() -- POST /v1/messages/count_tokens
    ///
    /// Returns the token count that the given request parameters would consume,
    /// without actually creating a message.
    pub async fn count_tokens(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, None).await
    }

    /// TS-style camelCase alias for [`Messages::count_tokens`].
    #[allow(non_snake_case)]
    pub async fn countTokens(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens(params).await
    }

    /// Count tokens with per-request options.
    pub async fn count_tokens_with_options(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageTokensCount, ApiError> {
        Ok(self
            .count_tokens_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        let body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!("failed to serialize MessageCountTokensParams: {e}"))
        })?;

        self.client
            .post_with_response("/v1/messages/count_tokens", &body, None, options)
            .await
    }

    /// TS-style camelCase alias for [`Messages::count_tokens_with_options`].
    #[allow(non_snake_case)]
    pub async fn countTokensWithOptions(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, options).await
    }

    /// TS-style camelCase alias for [`Messages::count_tokens_with_response`].
    #[allow(non_snake_case)]
    pub async fn countTokensWithResponse(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        self.count_tokens_with_response(params).await
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- serde skip unknown fields (TS: responses.test.ts) --

    #[test]
    fn serde_skip_unknown_fields_on_message() {
        // The API may return fields that the SDK does not yet model.
        // Serde's default behaviour (no `deny_unknown_fields`) should
        // silently skip them, matching the TS SDK behaviour.
        let json = serde_json::json!({
            "id": "msg_test123",
            "content": [],
            "model": "claude-opus-4-20250514",
            "role": "assistant",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "type": "message",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            },
            "some_future_field": "should be ignored",
            "another_unknown": 42
        });

        let msg: Message = serde_json::from_value(json)
            .expect("Message should deserialize successfully even with unknown fields");
        assert_eq!(msg.id, "msg_test123");
        assert_eq!(msg.model, "claude-opus-4-20250514");
        assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
    }

    #[test]
    fn serde_skip_unknown_fields_on_usage() {
        // Usage is a nested struct that may also gain new fields.
        let json = serde_json::json!({
            "input_tokens": 100,
            "output_tokens": 50,
            "new_future_token_type": 999
        });

        let usage: Usage = serde_json::from_value(json)
            .expect("Usage should deserialize successfully even with unknown fields");
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    // -- withResponse equivalent: request_id accessible through ApiError --

    #[test]
    fn api_error_generate_extracts_request_id() {
        // This mirrors the TS `withResponse` test which verifies
        // request_id is available on the response/error object.
        let mut headers = std::collections::HashMap::new();
        headers.insert("request-id".to_owned(), "req_xxx".to_owned());

        let err = crate::core::error::ApiError::generate(
            Some(500),
            Some(
                serde_json::json!({"type": "error", "error": {"type": "api_error", "message": "Internal error"}}),
            ),
            None,
            Some(headers),
        );

        assert_eq!(err.request_id(), Some("req_xxx"));
    }

    #[test]
    fn message_create_warning_messages_match_ts_deprecated_model_text() {
        let warnings = message_create_warning_messages("claude-instant-1.2", None, false);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0],
            "The model 'claude-instant-1.2' is deprecated and will reach end-of-life on November 6th, 2024\nPlease migrate to a newer model. Visit https://docs.anthropic.com/en/docs/resources/model-deprecations for more information."
        );

        assert!(message_create_warning_messages("claude-opus-4-0", None, false).is_empty());
    }

    #[test]
    fn message_create_warning_messages_match_ts_thinking_enabled_text() {
        let thinking = ThinkingConfig::Enabled {
            budget_tokens: 1024,
        };
        let warnings = message_create_warning_messages("claude-opus-4-6", Some(&thinking), false);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0],
            "Using Claude with claude-opus-4-6 and 'thinking.type=enabled' is deprecated. Use 'thinking.type=adaptive' instead which results in better model performance in our testing: https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking"
        );

        assert!(
            message_create_warning_messages(
                "claude-opus-4-6",
                Some(&ThinkingConfig::Adaptive),
                false
            )
            .is_empty()
        );
    }

    #[test]
    fn beta_warning_model_table_matches_ts_beta_messages() {
        assert!(
            message_create_warning_messages("claude-3-5-haiku-latest", None, false)
                .first()
                .unwrap()
                .contains("February 19th, 2026")
        );
        assert!(message_create_warning_messages("claude-3-5-haiku-latest", None, true).is_empty());
    }

    #[test]
    fn thinking_content_block_start_defaults_missing_signature_like_ts_json_parse() {
        // Mirrors grok/compat SSE: content_block_start omits signature; TS SDK
        // accepts via JSON.parse and fills later via signature_delta.
        let event: MessageStreamEvent = serde_json::from_value(serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {
                "type": "thinking",
                "thinking": ""
            }
        }))
        .expect("missing signature must deserialize with default \"\"");

        match event {
            MessageStreamEvent::ContentBlockStart {
                content_block:
                    ContentBlock::Thinking {
                        signature,
                        thinking,
                    },
                ..
            } => {
                assert_eq!(signature, "");
                assert_eq!(thinking, "");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn thinking_block_param_defaults_missing_signature_on_deserialize() {
        let param: ThinkingBlockParam = serde_json::from_value(serde_json::json!({
            "thinking": "internal reasoning"
        }))
        .expect("request param missing signature must default");
        assert_eq!(param.signature, "");
        assert_eq!(param.thinking, "internal reasoning");

        let roundtrip = serde_json::to_value(&param).unwrap();
        assert_eq!(roundtrip["signature"], "");
        assert_eq!(roundtrip["thinking"], "internal reasoning");
    }
}

// Maps to: TS resources/beta/messages/messages.ts

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::core::streaming::SseStream;
use crate::internal::request_options::RequestOptions;
use crate::resources::messages::{
    emit_message_create_warnings, CacheControlEphemeral, CacheCreation, CitationsConfigParam,
    JsonOutputFormat, Metadata, OutputConfig, SystemPrompt, TextBlock, TextCitation,
    TextCitationParam, ThinkingConfig, ToolChoice, ToolUnion, WebSearchToolResultBlockContent,
    WebSearchToolResultBlockParamContent,
};
use crate::sdk_lib::beta_message_stream::BetaMessageStream;
use crate::sdk_lib::beta_parser::{
    parse_beta_message, parsed_beta_message_without_parsing, ParsedBetaMessage,
};
use crate::sdk_lib::tools::{BetaToolRunner, BetaToolRunnerParams};

/// Rust representation of TS `unknown` JSON payloads in beta message types.
///
/// The TypeScript SDK intentionally leaves tool inputs and schema extension
/// objects as `unknown`. Rust keeps those semantically open while naming the
/// slots explicitly so audits can distinguish real untyped gaps from intentional
/// TS `unknown` parity.
pub type BetaUnknown = serde_json::Value;

/// Maps to TS tool/MCP `input: unknown` fields.
pub type BetaToolInput = BetaUnknown;

/// Maps to TS `BetaServerToolUseBlock.input` objects.
pub type BetaServerToolInput = HashMap<String, BetaUnknown>;

/// Maps to TS beta tool `input_examples` entries.
pub type BetaToolInputExample = HashMap<String, BetaUnknown>;

// TS exports beta-prefixed aliases for several stable shared message shapes.
// The wire schemas are identical, so Rust reuses the stable structs while
// exposing beta-prefixed public names for source-level parity.
pub type BetaCacheControlEphemeral = CacheControlEphemeral;
pub type BetaCacheCreation = CacheCreation;
pub type BetaCitationsConfigParam = CitationsConfigParam;
pub type BetaJSONOutputFormat = JsonOutputFormat;
pub type BetaMetadata = Metadata;
pub type BetaOutputConfig = OutputConfig;
pub type BetaTextCitation = TextCitation;
pub type BetaTextCitationParam = TextCitationParam;
pub type BetaThinkingConfigParam = ThinkingConfig;
pub type BetaThinkingConfigEnabled = ThinkingConfig;
pub type BetaThinkingConfigDisabled = ThinkingConfig;
pub type BetaThinkingConfigAdaptive = ThinkingConfig;
pub type BetaToolChoice = ToolChoice;
pub type BetaToolChoiceAny = ToolChoice;
pub type BetaToolChoiceAuto = ToolChoice;
pub type BetaToolChoiceNone = ToolChoice;
pub type BetaToolChoiceTool = ToolChoice;
pub type BetaWebSearchResultBlock = crate::resources::messages::WebSearchResultBlock;
pub type BetaWebSearchResultBlockParam = crate::resources::messages::WebSearchResultBlockParam;
pub type BetaWebSearchToolRequestError = crate::resources::messages::WebSearchToolRequestError;
pub type BetaWebSearchToolResultBlockContent = WebSearchToolResultBlockContent;
pub type BetaWebSearchToolResultBlockParamContent = WebSearchToolResultBlockParamContent;
pub type BetaWebSearchToolResultError = crate::resources::messages::WebSearchToolResultError;
pub type BetaWebSearchToolResultErrorCode = crate::resources::messages::WebSearchErrorCode;

// TS also exports beta-prefixed names for several singleton interfaces that
// Rust models as enum variants. These aliases keep the TS names available while
// preserving the idiomatic Rust enum constructors (for example,
// `BetaImageSource::Base64 { ... }` or `BetaContentBlockDelta::TextDelta`).
pub type BetaBase64ImageSource = BetaImageSource;
pub type BetaFileImageSource = BetaImageSource;
pub type BetaURLImageSource = BetaImageSource;
pub type BetaBase64PDFSource = BetaRequestDocumentSource;
pub type BetaFileDocumentSource = BetaRequestDocumentSource;
pub type BetaPlainTextSource = BetaRequestDocumentSource;
pub type BetaURLPDFSource = BetaRequestDocumentSource;
pub type BetaBase64PDFBlock = BetaRequestDocumentBlock;
pub type BetaTextDelta = BetaContentBlockDelta;
pub type BetaInputJSONDelta = BetaContentBlockDelta;
pub type BetaCitationsDelta = crate::resources::messages::CitationsDeltaData;
pub type BetaThinkingDelta = BetaContentBlockDelta;
pub type BetaSignatureDelta = BetaContentBlockDelta;
pub type BetaCompactionContentBlockDelta = BetaContentBlockDelta;
pub type BetaCitationCharLocation = TextCitation;
pub type BetaCitationPageLocation = TextCitation;
pub type BetaCitationContentBlockLocation = TextCitation;
pub type BetaCitationSearchResultLocation = TextCitation;
pub type BetaCitationsWebSearchResultLocation = TextCitation;
pub type BetaCitationCharLocationParam = TextCitationParam;
pub type BetaCitationPageLocationParam = TextCitationParam;
pub type BetaCitationContentBlockLocationParam = TextCitationParam;
pub type BetaCitationSearchResultLocationParam = TextCitationParam;
pub type BetaCitationWebSearchResultLocationParam = TextCitationParam;
pub type BetaCodeExecutionOutputBlockParam = BetaCodeExecutionOutputBlock;
pub type BetaCodeExecutionResultBlockParam = BetaCodeExecutionResultBlock;
pub type BetaCodeExecutionToolResultErrorParam = BetaCodeExecutionToolResultError;
pub type BetaBashCodeExecutionOutputBlockParam = BetaBashCodeExecutionOutputBlock;
pub type BetaBashCodeExecutionResultBlockParam = BetaBashCodeExecutionResultBlock;
pub type BetaBashCodeExecutionToolResultErrorParam = BetaBashCodeExecutionToolResultError;
pub type BetaTextEditorCodeExecutionCreateResultBlockParam =
    BetaTextEditorCodeExecutionCreateResultBlock;
pub type BetaTextEditorCodeExecutionStrReplaceResultBlockParam =
    BetaTextEditorCodeExecutionStrReplaceResultBlock;
pub type BetaTextEditorCodeExecutionViewResultBlockParam =
    BetaTextEditorCodeExecutionViewResultBlock;
pub type BetaTextEditorCodeExecutionToolResultErrorParam =
    BetaTextEditorCodeExecutionToolResultError;
pub type BetaWebFetchToolResultErrorBlockParam = BetaWebFetchToolResultErrorBlock;
pub type BetaDirectCaller = BetaToolCaller;
pub type BetaServerToolCaller = BetaToolCaller;
pub type BetaInputTokensClearAtLeast = BetaInputTokensThreshold;
pub type BetaInputTokensTrigger = BetaInputTokensThreshold;
pub type BetaToolUsesKeep = BetaToolUsesCount;
pub type BetaToolUsesTrigger = BetaToolUsesCount;
pub type BetaThinkingTurns = BetaThinkingKeep;
pub type BetaAllThinkingTurns = BetaThinkingKeep;
pub type BetaMessageIterationUsage = BetaIterationUsage;
pub type BetaCompactionIterationUsage = BetaIterationUsage;
pub type BetaContainerParams = BetaContainerParam;
pub type BetaRequestMCPServerURLDefinition = BetaMCPServerDefinition;
pub type BetaToolSearchToolBm25_20251119 = BetaToolSearchToolBm2520251119;
pub type BetaTextBlock = BetaContentBlock;
pub type BetaThinkingBlock = BetaContentBlock;
pub type BetaRedactedThinkingBlock = BetaContentBlock;
pub type BetaToolUseBlock = BetaContentBlock;
pub type BetaServerToolUseBlock = BetaContentBlock;
pub type BetaCodeExecutionToolResultBlock = BetaContentBlock;
pub type BetaBashCodeExecutionToolResultBlock = BetaContentBlock;
pub type BetaTextEditorCodeExecutionToolResultBlock = BetaContentBlock;
pub type BetaToolSearchToolResultBlock = BetaContentBlock;
pub type BetaWebFetchToolResultBlock = BetaContentBlock;
pub type BetaWebSearchToolResultBlock = BetaContentBlock;
pub type BetaContainerUploadBlock = BetaContentBlock;
pub type BetaRawContentBlockDelta = BetaContentBlockDelta;
pub type BetaRawMessageStreamEvent = BetaMessageStreamEvent;
pub type BetaRawMessageStartEvent = BetaMessageStreamEvent;
pub type BetaRawMessageDeltaEvent = BetaMessageStreamEvent;
pub type BetaRawMessageStopEvent = BetaMessageStreamEvent;
pub type BetaRawContentBlockStartEvent = BetaMessageStreamEvent;
pub type BetaRawContentBlockDeltaEvent = BetaMessageStreamEvent;
pub type BetaRawContentBlockStopEvent = BetaMessageStreamEvent;

// ─────────────────────────────────────────────────────────────────────────────
// Beta-specific stop reason
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaStopReason
///
/// Extends the base StopReason with beta-specific variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaStopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    PauseTurn,
    Compaction,
    Refusal,
    ModelContextWindowExceeded,
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta-specific block types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaMCPToolUseBlock
///
/// An MCP tool invocation generated by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolUseBlock {
    pub id: String,
    pub input: BetaToolInput,
    /// The name of the MCP tool.
    pub name: String,
    /// The name of the MCP server.
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaMCPToolUseBlock {
    fn default() -> Self {
        Self {
            id: String::new(),
            input: BetaUnknown::Null,
            name: String::new(),
            server_name: String::new(),
            type_name: "mcp_tool_use".to_owned(),
        }
    }
}

/// Maps to: TS BetaMCPToolResultBlock
///
/// The result of an MCP tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolResultBlock {
    pub content: BetaMCPToolResultContent,
    pub is_error: bool,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaMCPToolResultBlock.content
///
/// Content of an MCP tool result -- either a string or array of text blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMCPToolResultContent {
    Text(String),
    Blocks(Vec<TextBlock>),
}

/// Maps to: TS BetaCompactionBlock
///
/// A compaction block returned when autocompact is triggered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCompactionBlock {
    /// Summary of compacted content, or null if compaction failed.
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaCompactionBlock {
    fn default() -> Self {
        Self {
            content: None,
            type_name: "compaction".to_owned(),
        }
    }
}

/// Maps to: TS BetaDirectCaller / BetaServerToolCaller.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum BetaToolCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825 { tool_id: String },
}

/// Maps to: TS BetaServerToolUseBlock.name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaServerToolName {
    WebSearch,
    WebFetch,
    CodeExecution,
    BashCodeExecution,
    TextEditorCodeExecution,
    ToolSearchToolRegex,
    ToolSearchToolBm25,
}

/// Maps to: TS BetaCodeExecutionOutputBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaCodeExecutionResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionResultBlock {
    pub content: Vec<BetaCodeExecutionOutputBlock>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaCodeExecutionToolResultErrorCode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaCodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}

/// Maps to: TS BetaCodeExecutionToolResultError.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultError {
    pub error_code: BetaCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaCodeExecutionToolResultBlock.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaCodeExecutionToolResultBlockContent {
    Error(BetaCodeExecutionToolResultError),
    Result(BetaCodeExecutionResultBlock),
}

/// Maps to: TS BetaBashCodeExecutionOutputBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaBashCodeExecutionResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionResultBlock {
    pub content: Vec<BetaBashCodeExecutionOutputBlock>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaBashCodeExecutionToolResultError.error_code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaBashCodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    OutputFileTooLarge,
}

/// Maps to: TS BetaBashCodeExecutionToolResultError.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultError {
    pub error_code: BetaBashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaBashCodeExecutionToolResultBlock.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaBashCodeExecutionToolResultBlockContent {
    Error(BetaBashCodeExecutionToolResultError),
    Result(BetaBashCodeExecutionResultBlock),
}

/// Maps to: TS BetaTextEditorCodeExecutionToolResultError.error_code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaTextEditorCodeExecutionToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    FileNotFound,
}

/// Maps to: TS BetaTextEditorCodeExecutionToolResultError.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultError {
    pub error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
    pub error_message: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaTextEditorCodeExecutionViewResultBlock.file_type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaTextEditorCodeExecutionFileType {
    Text,
    Image,
    Pdf,
}

/// Maps to: TS BetaTextEditorCodeExecutionViewResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: BetaTextEditorCodeExecutionFileType,
    pub num_lines: Option<i64>,
    pub start_line: Option<i64>,
    pub total_lines: Option<i64>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaTextEditorCodeExecutionCreateResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionCreateResultBlock {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaTextEditorCodeExecutionStrReplaceResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionStrReplaceResultBlock {
    pub lines: Option<Vec<String>>,
    pub new_lines: Option<i64>,
    pub new_start: Option<i64>,
    pub old_lines: Option<i64>,
    pub old_start: Option<i64>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaTextEditorCodeExecutionToolResultBlock.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextEditorCodeExecutionToolResultBlockContent {
    Error(BetaTextEditorCodeExecutionToolResultError),
    View(BetaTextEditorCodeExecutionViewResultBlock),
    Create(BetaTextEditorCodeExecutionCreateResultBlock),
    StrReplace(BetaTextEditorCodeExecutionStrReplaceResultBlock),
}

/// Maps to: TS BetaToolReferenceBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolReferenceBlock {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaToolSearchToolResultError.error_code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaToolSearchToolResultErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}

/// Maps to: TS BetaToolSearchToolResultError.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultError {
    pub error_code: BetaToolSearchToolResultErrorCode,
    pub error_message: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaToolSearchToolSearchResultBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlock {
    pub tool_references: Vec<BetaToolReferenceBlock>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaToolSearchToolResultBlock.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockContent {
    Error(BetaToolSearchToolResultError),
    SearchResult(BetaToolSearchToolSearchResultBlock),
}

/// Maps to: TS BetaCitationConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCitationConfig {
    pub enabled: bool,
}

/// Maps to: TS BetaDocumentBlock.source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaDocumentSource {
    #[serde(rename = "base64")]
    Base64PDF { data: String, media_type: String },
    #[serde(rename = "text")]
    PlainText { data: String, media_type: String },
}

/// Maps to: TS BetaDocumentBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaDocumentBlock {
    pub citations: Option<BetaCitationConfig>,
    pub source: BetaDocumentSource,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaWebFetchBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchBlock {
    pub content: BetaDocumentBlock,
    pub retrieved_at: Option<String>,
    #[serde(rename = "type")]
    pub type_name: String,
    pub url: String,
}

/// Maps to: TS BetaWebFetchToolResultErrorCode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaWebFetchToolResultErrorCode {
    InvalidToolInput,
    UrlTooLong,
    UrlNotAllowed,
    UrlNotAccessible,
    UnsupportedContentType,
    TooManyRequests,
    MaxUsesExceeded,
    Unavailable,
}

/// Maps to: TS BetaWebFetchToolResultErrorBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlock {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaWebFetchToolResultBlock.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockContent {
    Error(BetaWebFetchToolResultErrorBlock),
    Result(BetaWebFetchBlock),
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaContentBlock
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaContentBlock
///
/// Extends the base ContentBlock with beta-specific variants such as
/// MCP tool use/result and compaction blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContentBlock {
    /// Maps to: TS BetaTextBlock
    #[serde(rename = "text")]
    Text {
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<TextCitation>>,
        text: String,
    },

    /// Maps to: TS BetaThinkingBlock
    ///
    /// `signature` defaults to `""` so SSE `content_block_start` events that omit
    /// the field (filled later by `signature_delta`) match the TS SDK runtime.
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        signature: String,
        thinking: String,
    },

    /// Maps to: TS BetaRedactedThinkingBlock
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },

    /// Maps to: TS BetaToolUseBlock
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        input: BetaToolInput,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<BetaToolCaller>,
    },

    /// Maps to: TS BetaServerToolUseBlock
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<BetaToolCaller>,
        id: String,
        input: BetaServerToolInput,
        name: BetaServerToolName,
    },

    /// Maps to: TS BetaWebSearchToolResultBlock
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        content: WebSearchToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaWebFetchToolResultBlock
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult {
        content: BetaWebFetchToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaCodeExecutionToolResultBlock
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult {
        content: BetaCodeExecutionToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaBashCodeExecutionToolResultBlock
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult {
        content: BetaBashCodeExecutionToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaTextEditorCodeExecutionToolResultBlock
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult {
        content: BetaTextEditorCodeExecutionToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaToolSearchToolResultBlock
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult {
        content: BetaToolSearchToolResultBlockContent,
        tool_use_id: String,
    },

    /// Maps to: TS BetaMCPToolUseBlock
    #[serde(rename = "mcp_tool_use")]
    McpToolUse {
        id: String,
        input: BetaToolInput,
        name: String,
        server_name: String,
    },

    /// Maps to: TS BetaMCPToolResultBlock
    #[serde(rename = "mcp_tool_result")]
    McpToolResult {
        content: BetaMCPToolResultContent,
        is_error: bool,
        tool_use_id: String,
    },

    /// Maps to: TS BetaContainerUploadBlock
    #[serde(rename = "container_upload")]
    ContainerUpload { file_id: String },

    /// Maps to: TS BetaCompactionBlock
    #[serde(rename = "compaction")]
    Compaction { content: Option<String> },
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaContentBlockParam (request-side)
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaTextBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextBlockParam {
    pub text: String,
    /// SDK-helper markers used only for `x-stainless-helper`; not serialized.
    #[serde(skip)]
    pub stainless_helpers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<TextCitationParam>>,
}

/// Maps to: TS BetaImageBlockParam.source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaImageSource {
    #[serde(rename = "base64")]
    Base64 { data: String, media_type: String },
    #[serde(rename = "url")]
    Url { url: String },
    #[serde(rename = "file")]
    File { file_id: String },
}

/// Maps to: TS BetaImageBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaImageBlockParam {
    pub source: BetaImageSource,
    /// SDK-helper markers used only for `x-stainless-helper`; not serialized.
    #[serde(skip)]
    pub stainless_helpers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaContentBlockSourceContent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContentBlockSourceContent {
    #[serde(rename = "text")]
    Text(BetaTextBlockParam),
    #[serde(rename = "image")]
    Image(BetaImageBlockParam),
}

/// Maps to: TS BetaContentBlockSource.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockSourceData {
    Text(String),
    Blocks(Vec<BetaContentBlockSourceContent>),
}

/// Maps to: TS BetaContentBlockSource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContentBlockSource {
    pub content: BetaContentBlockSourceData,
}

/// Maps to: TS BetaRequestDocumentBlock.source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaRequestDocumentSource {
    #[serde(rename = "base64")]
    Base64PDF { data: String, media_type: String },
    #[serde(rename = "text")]
    PlainText { data: String, media_type: String },
    #[serde(rename = "content")]
    Content { content: BetaContentBlockSourceData },
    #[serde(rename = "url")]
    UrlPDF { url: String },
    #[serde(rename = "file")]
    File { file_id: String },
}

/// Maps to: TS BetaRequestDocumentBlock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestDocumentBlock {
    pub source: BetaRequestDocumentSource,
    /// SDK-helper markers used only for `x-stainless-helper`; not serialized.
    #[serde(skip)]
    pub stainless_helpers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Maps to: TS BetaSearchResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSearchResultBlockParam {
    pub content: Vec<BetaTextBlockParam>,
    pub source: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,
}

/// Maps to: TS BetaThinkingBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaThinkingBlockParam {
    /// Defaults to `""` when absent (TS runtime accepts missing then fills later).
    #[serde(default)]
    pub signature: String,
    pub thinking: String,
}

/// Maps to: TS BetaRedactedThinkingBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRedactedThinkingBlockParam {
    pub data: String,
}

/// Maps to: TS BetaToolUseBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUseBlockParam {
    pub id: String,
    pub input: BetaToolInput,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

/// Maps to: TS BetaToolReferenceBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolReferenceBlockParam {
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaToolResultContentBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaToolResultContentBlockParam {
    #[serde(rename = "text")]
    Text(BetaTextBlockParam),
    #[serde(rename = "image")]
    Image(BetaImageBlockParam),
    #[serde(rename = "search_result")]
    SearchResult(BetaSearchResultBlockParam),
    #[serde(rename = "document")]
    Document(BetaRequestDocumentBlock),
    #[serde(rename = "tool_reference")]
    ToolReference(BetaToolReferenceBlockParam),
}

/// Maps to: TS BetaToolResultBlockParam.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolResultContent {
    Text(String),
    Blocks(Vec<BetaToolResultContentBlockParam>),
}

/// Maps to: TS BetaToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Maps to: TS BetaServerToolUseBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaServerToolUseBlockParam {
    pub id: String,
    pub input: BetaToolInput,
    pub name: BetaServerToolName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

/// Maps to: TS BetaWebSearchToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlockParam {
    pub content: BetaWebSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaWebFetchBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchBlockParam {
    pub content: BetaRequestDocumentBlock,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved_at: Option<String>,
}

/// Maps to: TS BetaWebFetchToolResultBlockParam.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockParamContent {
    Error(BetaWebFetchToolResultErrorBlock),
    Result(BetaWebFetchBlockParam),
}

/// Maps to: TS BetaWebFetchToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlockParam {
    pub content: BetaWebFetchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

pub type BetaCodeExecutionToolResultBlockParamContent = BetaCodeExecutionToolResultBlockContent;

/// Maps to: TS BetaCodeExecutionToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlockParam {
    pub content: BetaCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

pub type BetaBashCodeExecutionToolResultBlockParamContent =
    BetaBashCodeExecutionToolResultBlockContent;

/// Maps to: TS BetaBashCodeExecutionToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlockParam {
    pub content: BetaBashCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

pub type BetaTextEditorCodeExecutionToolResultBlockParamContent =
    BetaTextEditorCodeExecutionToolResultBlockContent;

/// Maps to: TS BetaTextEditorCodeExecutionToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlockParam {
    pub content: BetaTextEditorCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaToolReferenceBlockParam used by tool-search result params.
pub type BetaToolSearchToolReferenceBlockParam = BetaToolReferenceBlockParam;

/// Maps to: TS BetaToolSearchToolResultErrorParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultErrorParam {
    pub error_code: BetaToolSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaToolSearchToolSearchResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlockParam {
    pub tool_references: Vec<BetaToolSearchToolReferenceBlockParam>,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaToolSearchToolResultBlockParam.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockParamContent {
    Error(BetaToolSearchToolResultErrorParam),
    SearchResult(BetaToolSearchToolSearchResultBlockParam),
}

/// Maps to: TS BetaToolSearchToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlockParam {
    pub content: BetaToolSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaMCPToolUseBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolUseBlockParam {
    pub id: String,
    pub input: BetaToolInput,
    pub name: String,
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaRequestMCPToolResultBlockParam.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaRequestMCPToolResultContent {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

/// Maps to: TS BetaRequestMCPToolResultBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestMCPToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaRequestMCPToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Maps to: TS BetaContainerUploadBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainerUploadBlockParam {
    pub file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaCompactionBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCompactionBlockParam {
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
}

/// Maps to: TS BetaContentBlockParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContentBlockParam {
    #[serde(rename = "text")]
    Text(BetaTextBlockParam),
    #[serde(rename = "image")]
    Image(BetaImageBlockParam),
    #[serde(rename = "document")]
    Document(BetaRequestDocumentBlock),
    #[serde(rename = "search_result")]
    SearchResult(BetaSearchResultBlockParam),
    #[serde(rename = "thinking")]
    Thinking(BetaThinkingBlockParam),
    #[serde(rename = "redacted_thinking")]
    RedactedThinking(BetaRedactedThinkingBlockParam),
    #[serde(rename = "tool_use")]
    ToolUse(BetaToolUseBlockParam),
    #[serde(rename = "tool_result")]
    ToolResult(BetaToolResultBlockParam),
    #[serde(rename = "server_tool_use")]
    ServerToolUse(BetaServerToolUseBlockParam),
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult(BetaWebSearchToolResultBlockParam),
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult(BetaWebFetchToolResultBlockParam),
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult(BetaCodeExecutionToolResultBlockParam),
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult(BetaBashCodeExecutionToolResultBlockParam),
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult(BetaTextEditorCodeExecutionToolResultBlockParam),
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult(BetaToolSearchToolResultBlockParam),
    #[serde(rename = "mcp_tool_use")]
    McpToolUse(BetaMCPToolUseBlockParam),
    #[serde(rename = "mcp_tool_result")]
    McpToolResult(BetaRequestMCPToolResultBlockParam),
    #[serde(rename = "container_upload")]
    ContainerUpload(BetaContainerUploadBlockParam),
    #[serde(rename = "compaction")]
    Compaction(BetaCompactionBlockParam),
}

/// Maps to: TS BetaMessageParam.content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMessageContent {
    Text(String),
    Blocks(Vec<BetaContentBlockParam>),
}

/// Maps to: TS BetaMessageParam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageParam {
    pub content: BetaMessageContent,
    pub role: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta usage
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaServerToolUsage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaServerToolUsage {
    pub web_fetch_requests: i64,
    pub web_search_requests: i64,
}

/// Maps to: TS BetaMessageIterationUsage / BetaCompactionIterationUsage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaIterationUsage {
    #[serde(rename = "message")]
    Message {
        cache_creation: Option<CacheCreation>,
        cache_creation_input_tokens: i64,
        cache_read_input_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
    },
    #[serde(rename = "compaction")]
    Compaction {
        cache_creation: Option<CacheCreation>,
        cache_creation_input_tokens: i64,
        cache_read_input_tokens: i64,
        input_tokens: i64,
        output_tokens: i64,
    },
}

/// Maps to: TS BetaIterationsUsage.
pub type BetaIterationsUsage = Vec<BetaIterationUsage>;

/// Maps to: TS BetaUsage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<CacheCreation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,
    pub input_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<BetaIterationsUsage>,
    pub output_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<BetaServerToolUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Context management
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaInputTokensClearAtLeast / BetaInputTokensTrigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaInputTokensThreshold {
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: i64,
}

impl BetaInputTokensThreshold {
    pub fn input_tokens(value: i64) -> Self {
        Self {
            type_name: "input_tokens".to_owned(),
            value,
        }
    }
}

/// Maps to: TS BetaToolUsesKeep / BetaToolUsesTrigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUsesCount {
    #[serde(rename = "type")]
    pub type_name: String,
    pub value: i64,
}

impl BetaToolUsesCount {
    pub fn tool_uses(value: i64) -> Self {
        Self {
            type_name: "tool_uses".to_owned(),
            value,
        }
    }
}

/// Maps to: TS BetaThinkingTurns / BetaAllThinkingTurns / 'all'.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaThinkingKeep {
    ThinkingTurns {
        #[serde(rename = "type")]
        type_name: String,
        value: i64,
    },
    AllObject {
        #[serde(rename = "type")]
        type_name: String,
    },
    AllString(String),
}

impl BetaThinkingKeep {
    pub fn thinking_turns(value: i64) -> Self {
        Self::ThinkingTurns {
            type_name: "thinking_turns".to_owned(),
            value,
        }
    }

    pub fn all() -> Self {
        Self::AllString("all".to_owned())
    }
}

/// Maps to: TS BetaClearToolUses20250919Edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919Edit {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_at_least: Option<BetaInputTokensThreshold>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_tool_inputs: Option<BetaClearToolInputs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<BetaToolUsesCount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<BetaContextManagementTrigger>,
}

impl Default for BetaClearToolUses20250919Edit {
    fn default() -> Self {
        Self {
            type_name: "clear_tool_uses_20250919".to_owned(),
            clear_at_least: None,
            clear_tool_inputs: None,
            exclude_tools: None,
            keep: None,
            trigger: None,
        }
    }
}

/// Maps to: TS `boolean | Array<string>` for `clear_tool_inputs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearToolInputs {
    All(bool),
    ToolNames(Vec<String>),
}

/// Maps to: TS BetaInputTokensTrigger | BetaToolUsesTrigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementTrigger {
    InputTokens(BetaInputTokensThreshold),
    ToolUses(BetaToolUsesCount),
}

/// Maps to: TS BetaClearThinking20251015Edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearThinking20251015Edit {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<BetaThinkingKeep>,
}

impl Default for BetaClearThinking20251015Edit {
    fn default() -> Self {
        Self {
            type_name: "clear_thinking_20251015".to_owned(),
            keep: None,
        }
    }
}

/// Maps to: TS BetaCompact20260112Edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCompact20260112Edit {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pause_after_compaction: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<BetaInputTokensThreshold>,
}

impl Default for BetaCompact20260112Edit {
    fn default() -> Self {
        Self {
            type_name: "compact_20260112".to_owned(),
            instructions: None,
            pause_after_compaction: None,
            trigger: None,
        }
    }
}

/// Maps to: TS context management edit union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContextManagementEdit {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919 {
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_at_least: Option<BetaInputTokensThreshold>,
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_tool_inputs: Option<BetaClearToolInputs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exclude_tools: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<BetaToolUsesCount>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger: Option<BetaContextManagementTrigger>,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015 {
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<BetaThinkingKeep>,
    },
    #[serde(rename = "compact_20260112")]
    Compact20260112 {
        #[serde(skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pause_after_compaction: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger: Option<BetaInputTokensThreshold>,
    },
}

/// Maps to: TS BetaClearToolUses20250919EditResponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919EditResponse {
    pub cleared_input_tokens: i64,
    pub cleared_tool_uses: i64,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS BetaClearThinking20251015EditResponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearThinking20251015EditResponse {
    pub cleared_input_tokens: i64,
    pub cleared_thinking_turns: i64,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS applied context management edit union.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContextManagementAppliedEdit {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919 {
        cleared_input_tokens: i64,
        cleared_tool_uses: i64,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015 {
        cleared_input_tokens: i64,
        cleared_thinking_turns: i64,
    },
}

/// Maps to: TS BetaContextManagementResponse.
///
/// Information about context management strategies applied during the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContextManagementResponse {
    /// List of context management edits that were applied.
    pub applied_edits: Vec<BetaContextManagementAppliedEdit>,
}

/// Maps to: TS BetaCountTokensContextManagementResponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCountTokensContextManagementResponse {
    pub original_input_tokens: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaContainer
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaSkill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSkill {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub version: String,
}

/// Maps to: TS BetaSkillParams.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSkillParams {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Maps to: TS BetaContainer
///
/// Information about the container used in the request (for the code execution tool).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainer {
    /// Identifier for the container used in this request.
    pub id: String,
    /// The time at which the container will expire.
    pub expires_at: String,
    /// Skills loaded in the container.
    pub skills: Option<Vec<BetaSkill>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaMessage
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaMessage
///
/// A message response from the beta Messages API. Extends the base Message
/// with fields for container, context management, and beta-specific stop
/// reasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessage {
    /// Unique object identifier.
    pub id: String,

    /// Request id from the `request-id` response header.
    #[serde(rename = "_request_id", default, skip_serializing)]
    pub request_id: Option<String>,

    /// Information about the container used in the request.
    pub container: Option<BetaContainer>,

    /// Content generated by the model.
    pub content: Vec<BetaContentBlock>,

    /// Context management response.
    pub context_management: Option<BetaContextManagementResponse>,

    /// The model that handled the request.
    pub model: String,

    /// Conversational role. Always `"assistant"`.
    pub role: String,

    /// The reason that we stopped.
    pub stop_reason: Option<BetaStopReason>,

    /// Which custom stop sequence was generated, if any.
    pub stop_sequence: Option<String>,

    /// Object type. Always `"message"`.
    #[serde(rename = "type")]
    pub type_name: String,

    /// Billing and rate-limit usage.
    pub usage: BetaUsage,
}

impl Default for BetaMessage {
    fn default() -> Self {
        Self {
            id: String::new(),
            request_id: None,
            container: None,
            content: vec![],
            context_management: None,
            model: String::new(),
            role: "assistant".to_owned(),
            stop_reason: None,
            stop_sequence: None,
            type_name: "message".to_owned(),
            usage: BetaUsage::default(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta token count
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaMessageTokensCount
///
/// Token count result from the beta count_tokens endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageTokensCount {
    /// Information about context management applied to the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaCountTokensContextManagementResponse>,

    /// The total number of tokens across the provided messages, system
    /// prompt, and tools.
    pub input_tokens: i64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta MCP toolset params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaRequestMCPServerToolConfiguration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRequestMCPServerToolConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Maps to: TS BetaRequestMCPServerURLDefinition
///
/// MCP server definition for use in beta message requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPServerDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<BetaRequestMCPServerToolConfiguration>,
}

/// Maps to: TS beta tool `allowed_callers` values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BetaToolAllowedCaller {
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

/// Extra example objects accepted by beta server-managed tool definitions.
pub type BetaToolInputExamples = Vec<BetaToolInputExample>;

/// Maps to: TS BetaTool.InputSchema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolInputSchema {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BetaUnknown>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(flatten)]
    pub additional_properties: HashMap<String, BetaUnknown>,
}

impl Default for BetaToolInputSchema {
    fn default() -> Self {
        Self {
            type_name: "object".to_owned(),
            properties: None,
            required: None,
            additional_properties: HashMap::new(),
        }
    }
}

/// Rust best-effort alias for the TS nested name `BetaTool.InputSchema`.
pub type InputSchema = BetaToolInputSchema;

/// Maps to: TS BetaTool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTool {
    pub input_schema: BetaToolInputSchema,
    pub name: String,
    /// SDK-helper marker used only for building the `x-stainless-helper`
    /// request header. It is never serialized into the request body.
    #[serde(skip)]
    pub stainless_helper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
}

/// Maps to: TS BetaToolBash20241022.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolBash20241022 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolBash20241022 {
    fn default() -> Self {
        Self {
            name: "bash".to_owned(),
            type_name: "bash_20241022".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolBash20250124.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolBash20250124 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolBash20250124 {
    fn default() -> Self {
        Self {
            name: "bash".to_owned(),
            type_name: "bash_20250124".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolComputerUse20241022.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20241022 {
    pub display_height_px: i64,
    pub display_width_px: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Maps to: TS BetaToolComputerUse20250124.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20250124 {
    pub display_height_px: i64,
    pub display_width_px: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Maps to: TS BetaToolComputerUse20251124.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolComputerUse20251124 {
    pub display_height_px: i64,
    pub display_width_px: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Maps to: TS BetaCodeExecutionTool20250522.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250522 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaCodeExecutionTool20250522 {
    fn default() -> Self {
        Self {
            name: "code_execution".to_owned(),
            type_name: "code_execution_20250522".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaCodeExecutionTool20250825.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250825 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaCodeExecutionTool20250825 {
    fn default() -> Self {
        Self {
            name: "code_execution".to_owned(),
            type_name: "code_execution_20250825".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaMemoryTool20250818Command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "command")]
pub enum BetaMemoryTool20250818Command {
    #[serde(rename = "view")]
    View(BetaMemoryTool20250818ViewCommand),
    #[serde(rename = "create")]
    Create(BetaMemoryTool20250818CreateCommand),
    #[serde(rename = "str_replace")]
    StrReplace(BetaMemoryTool20250818StrReplaceCommand),
    #[serde(rename = "insert")]
    Insert(BetaMemoryTool20250818InsertCommand),
    #[serde(rename = "delete")]
    Delete(BetaMemoryTool20250818DeleteCommand),
    #[serde(rename = "rename")]
    Rename(BetaMemoryTool20250818RenameCommand),
}

impl BetaMemoryTool20250818Command {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::View(_) => "view",
            Self::Create(_) => "create",
            Self::StrReplace(_) => "str_replace",
            Self::Insert(_) => "insert",
            Self::Delete(_) => "delete",
            Self::Rename(_) => "rename",
        }
    }
}

/// Maps to: TS BetaMemoryTool20250818ViewCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818ViewCommand {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_range: Option<Vec<i64>>,
}

/// Maps to: TS BetaMemoryTool20250818CreateCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818CreateCommand {
    pub file_text: String,
    pub path: String,
}

/// Maps to: TS BetaMemoryTool20250818StrReplaceCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818StrReplaceCommand {
    pub new_str: String,
    pub old_str: String,
    pub path: String,
}

/// Maps to: TS BetaMemoryTool20250818InsertCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818InsertCommand {
    pub insert_line: i64,
    pub insert_text: String,
    pub path: String,
}

/// Maps to: TS BetaMemoryTool20250818DeleteCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818DeleteCommand {
    pub path: String,
}

/// Maps to: TS BetaMemoryTool20250818RenameCommand.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BetaMemoryTool20250818RenameCommand {
    pub new_path: String,
    pub old_path: String,
}

/// Maps to: TS BetaMemoryTool20250818.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMemoryTool20250818 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaMemoryTool20250818 {
    fn default() -> Self {
        Self {
            name: "memory".to_owned(),
            type_name: "memory_20250818".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolTextEditor20241022.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20241022 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolTextEditor20241022 {
    fn default() -> Self {
        Self {
            name: "str_replace_editor".to_owned(),
            type_name: "text_editor_20241022".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolTextEditor20250124.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250124 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolTextEditor20250124 {
    fn default() -> Self {
        Self {
            name: "str_replace_editor".to_owned(),
            type_name: "text_editor_20250124".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolTextEditor20250429.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250429 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolTextEditor20250429 {
    fn default() -> Self {
        Self {
            name: "str_replace_based_edit_tool".to_owned(),
            type_name: "text_editor_20250429".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolTextEditor20250728.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250728 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<BetaToolInputExamples>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolTextEditor20250728 {
    fn default() -> Self {
        Self {
            name: "str_replace_based_edit_tool".to_owned(),
            type_name: "text_editor_20250728".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            input_examples: None,
            max_characters: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaWebSearchTool20250305.UserLocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolUserLocation {
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

impl Default for BetaWebSearchToolUserLocation {
    fn default() -> Self {
        Self {
            type_name: "approximate".to_owned(),
            city: None,
            country: None,
            region: None,
            timezone: None,
        }
    }
}

/// Rust alias for TS `BetaWebSearchTool20250305.UserLocation`.
pub type BetaWebSearchTool20250305UserLocation = BetaWebSearchToolUserLocation;

/// Rust best-effort alias for the TS nested name `UserLocation`.
pub type UserLocation = BetaWebSearchToolUserLocation;

/// Maps to: TS BetaWebSearchTool20250305.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchTool20250305 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<BetaWebSearchToolUserLocation>,
}

impl Default for BetaWebSearchTool20250305 {
    fn default() -> Self {
        Self {
            name: "web_search".to_owned(),
            type_name: "web_search_20250305".to_owned(),
            allowed_callers: None,
            allowed_domains: None,
            blocked_domains: None,
            cache_control: None,
            defer_loading: None,
            max_uses: None,
            strict: None,
            user_location: None,
        }
    }
}

/// Maps to: TS BetaToolSearchToolBm25_20251119.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolBm2520251119 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolSearchToolBm2520251119 {
    fn default() -> Self {
        Self {
            name: "tool_search_tool_bm25".to_owned(),
            type_name: "tool_search_tool_bm25_20251119".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolSearchToolRegex20251119.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolRegex20251119 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaToolSearchToolRegex20251119 {
    fn default() -> Self {
        Self {
            name: "tool_search_tool_regex".to_owned(),
            type_name: "tool_search_tool_regex_20251119".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaMCPToolConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Maps to: TS BetaMCPToolDefaultConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolDefaultConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Maps to: TS BetaMCPToolset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolset {
    pub mcp_server_name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<HashMap<String, BetaMCPToolConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<BetaMCPToolDefaultConfig>,
}

/// Maps to: TS BetaWebFetchTool20250910.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchTool20250910 {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlEphemeral>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

impl Default for BetaWebFetchTool20250910 {
    fn default() -> Self {
        Self {
            name: "web_fetch".to_owned(),
            type_name: "web_fetch_20250910".to_owned(),
            allowed_callers: None,
            allowed_domains: None,
            blocked_domains: None,
            cache_control: None,
            citations: None,
            defer_loading: None,
            max_content_tokens: None,
            max_uses: None,
            strict: None,
        }
    }
}

/// Maps to: TS BetaToolUnion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolUnion {
    Custom(BetaTool),
    Bash20241022(BetaToolBash20241022),
    Bash20250124(BetaToolBash20250124),
    CodeExecution20250522(BetaCodeExecutionTool20250522),
    CodeExecution20250825(BetaCodeExecutionTool20250825),
    ComputerUse20241022(BetaToolComputerUse20241022),
    Memory20250818(BetaMemoryTool20250818),
    ComputerUse20250124(BetaToolComputerUse20250124),
    TextEditor20241022(BetaToolTextEditor20241022),
    ComputerUse20251124(BetaToolComputerUse20251124),
    TextEditor20250124(BetaToolTextEditor20250124),
    TextEditor20250429(BetaToolTextEditor20250429),
    TextEditor20250728(BetaToolTextEditor20250728),
    WebSearch20250305(BetaWebSearchTool20250305),
    WebFetch20250910(BetaWebFetchTool20250910),
    ToolSearchBm2520251119(BetaToolSearchToolBm2520251119),
    ToolSearchRegex20251119(BetaToolSearchToolRegex20251119),
    McpToolset(BetaMCPToolset),
    Base(ToolUnion),
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaContainer
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaContainerParams
///
/// Container parameters for beta message requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContainerParam {
    Id(String),
    Params {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        skills: Option<Vec<BetaSkillParams>>,
    },
}

/// Maps to: TS BetaContextManagementConfig
///
/// Configuration for context management in beta requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContextManagementConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<BetaContextManagementEdit>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaMessageCreateParams
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/messages MessageCreateParams
///
/// Parameters for creating a beta message. Extends the base
/// MessageCreateParams with beta-specific fields like `betas`,
/// `mcp_servers`, `container`, and `context_management`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaMessageCreateParams {
    /// The maximum number of tokens to generate before stopping.
    pub max_tokens: i64,

    /// Input messages.
    pub messages: Vec<BetaMessageParam>,

    /// The model to use.
    pub model: String,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    /// This field is not serialized into the request body.
    #[serde(skip)]
    pub betas: Option<Vec<String>>,

    /// Container identifier or parameters for code execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainerParam>,

    /// Context management configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,

    /// Specifies the geographic region for inference processing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_geo: Option<String>,

    /// MCP servers to be utilized in this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaMCPServerDefinition>>,

    /// An object describing metadata about the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,

    /// Configuration options for the model's output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,

    /// Deprecated: use `output_config.format` instead.
    ///
    /// Maps to: TS `MessageCreateParamsBase.output_format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<JsonOutputFormat>,

    /// Service tier preference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,

    /// The inference speed mode for this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,

    /// Custom text sequences that will cause the model to stop generating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Whether to incrementally stream the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    /// System prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,

    /// Amount of randomness injected into the response (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Configuration for enabling Claude's extended thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    /// How the model should use the provided tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Definitions of tools that the model may use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,

    /// Only sample from the top K options for each subsequent token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,

    /// Use nucleus sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

/// TS beta namespace export-name alias for `MessageCreateParams`.
pub type MessageCreateParams = BetaMessageCreateParams;
/// TS beta namespace export-name alias for `MessageCreateParamsBase`.
pub type MessageCreateParamsBase = BetaMessageCreateParams;
/// TS beta namespace export-name alias for `MessageCreateParamsNonStreaming`.
pub type MessageCreateParamsNonStreaming = BetaMessageCreateParams;
/// TS beta namespace export-name alias for `MessageCreateParamsStreaming`.
pub type MessageCreateParamsStreaming = BetaMessageCreateParams;

/// Maps to: TS `BetaMessageStreamParams = MessageCreateParamsBase`.
///
/// Rust uses one create-param struct for both non-streaming and streaming beta
/// message requests; `create_stream()`/`stream()` force `stream: true` on the
/// wire just like the TS helper methods.
pub type BetaMessageStreamParams = BetaMessageCreateParams;

// ─────────────────────────────────────────────────────────────────────────────
// BetaMessageCountTokensParams
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/messages MessageCountTokensParams
///
/// Parameters for counting tokens in a beta message request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaMessageCountTokensParams {
    /// Input messages.
    pub messages: Vec<BetaMessageParam>,

    /// The model to use.
    pub model: String,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip)]
    pub betas: Option<Vec<String>>,

    /// Context management configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,

    /// MCP servers to be utilized in this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaMCPServerDefinition>>,

    /// Configuration options for the model's output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,

    /// Deprecated: use `output_config.format` instead.
    ///
    /// Maps to: TS `MessageCountTokensParams.output_format`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<JsonOutputFormat>,

    /// The inference speed mode for this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,

    /// System prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,

    /// Configuration for enabling Claude's extended thinking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,

    /// How the model should use the provided tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,

    /// Definitions of tools that the model may use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,
}

/// TS beta namespace export-name alias for `MessageCountTokensParams`.
pub type MessageCountTokensParams = BetaMessageCountTokensParams;

// ─────────────────────────────────────────────────────────────────────────────
// Beta stream event types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaRawMessageDeltaEvent.Delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

/// Rust alias for TS `BetaRawMessageDeltaEvent.Delta`.
pub type BetaRawMessageDeltaEventDelta = BetaMessageDelta;

/// Rust best-effort alias for the TS nested name `Delta`.
pub type Delta = BetaMessageDelta;

/// Maps to: TS BetaMessageDeltaUsage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageDeltaUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterations: Option<BetaIterationsUsage>,
    pub output_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<BetaServerToolUsage>,
}

/// Maps to: TS BetaRawContentBlockDelta.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContentBlockDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "citations_delta")]
    CitationsDelta { citation: TextCitation },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
    #[serde(rename = "compaction_delta")]
    CompactionDelta { content: Option<String> },
}

/// Maps to: TS BetaRawMessageStreamEvent
///
/// Events emitted during beta streaming message creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaMessageStreamEvent {
    /// Maps to: TS BetaRawMessageStartEvent
    #[serde(rename = "message_start")]
    MessageStart { message: Box<BetaMessage> },

    /// Maps to: TS BetaRawMessageDeltaEvent
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: BetaMessageDelta,
        usage: BetaMessageDeltaUsage,
        #[serde(skip_serializing_if = "Option::is_none")]
        context_management: Option<BetaContextManagementResponse>,
    },

    /// Maps to: TS BetaRawMessageStopEvent
    #[serde(rename = "message_stop")]
    MessageStop,

    /// Maps to: TS BetaRawContentBlockStartEvent
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        content_block: BetaContentBlock,
        index: usize,
    },

    /// Maps to: TS BetaRawContentBlockDeltaEvent
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        delta: BetaContentBlockDelta,
        index: usize,
    },

    /// Maps to: TS BetaRawContentBlockStopEvent
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: usize },

    /// Ping event for keep-alive.
    #[serde(rename = "ping")]
    Ping,
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/messages Messages class
///
/// Resource for creating messages through the beta Messages API.
fn has_beta_json_schema_output_format(params: &BetaMessageCreateParams) -> bool {
    params
        .output_format
        .as_ref()
        .or_else(|| {
            params
                .output_config
                .as_ref()
                .and_then(|config| config.format.as_ref())
        })
        .is_some_and(|format| format.type_name == "json_schema")
}

fn beta_message_headers(
    betas: Option<&[String]>,
    tools: Option<&[BetaToolUnion]>,
    messages: &[BetaMessageParam],
) -> Option<HashMap<String, Option<String>>> {
    let mut headers = HashMap::new();

    if let Some(betas) = betas {
        headers.insert("anthropic-beta".to_owned(), Some(betas.join(",")));
    }

    let helpers = collect_beta_stainless_helpers(tools, messages);
    if !helpers.is_empty() {
        headers.insert("x-stainless-helper".to_owned(), Some(helpers.join(", ")));
    }

    if headers.is_empty() {
        None
    } else {
        Some(headers)
    }
}

fn collect_beta_stainless_helpers(
    tools: Option<&[BetaToolUnion]>,
    messages: &[BetaMessageParam],
) -> Vec<String> {
    let mut helpers = Vec::new();

    if let Some(tools) = tools {
        for tool in tools {
            if let Some(helper) = beta_tool_stainless_helper(tool) {
                push_unique_helper(&mut helpers, helper);
            }
        }
    }

    for message in messages {
        if let BetaMessageContent::Blocks(blocks) = &message.content {
            for block in blocks {
                for helper in beta_content_block_stainless_helpers(block) {
                    push_unique_helper(&mut helpers, helper);
                }
            }
        }
    }

    helpers
}

fn push_unique_helper(helpers: &mut Vec<String>, helper: &str) {
    if !helpers.iter().any(|existing| existing == helper) {
        helpers.push(helper.to_owned());
    }
}

fn beta_tool_stainless_helper(tool: &BetaToolUnion) -> Option<&str> {
    match tool {
        BetaToolUnion::Custom(tool) => tool.stainless_helper.as_deref(),
        _ => None,
    }
}

fn beta_content_block_stainless_helpers(block: &BetaContentBlockParam) -> &[String] {
    match block {
        BetaContentBlockParam::Text(block) => &block.stainless_helpers,
        BetaContentBlockParam::Image(block) => &block.stainless_helpers,
        BetaContentBlockParam::Document(block) => &block.stainless_helpers,
        _ => &[],
    }
}

pub struct BetaMessages<'a> {
    client: &'a Anthropic,
}

/// TS beta namespace export-name alias for the `Messages` resource class.
pub type Messages<'a> = BetaMessages<'a>;

impl<'a> BetaMessages<'a> {
    /// Create a new `BetaMessages` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Access the beta message Batches sub-resource.
    pub fn batches(&self) -> super::batches::BetaBatches<'a> {
        super::batches::BetaBatches::new(self.client)
    }

    /// Maps to: TS Messages.create() -- POST /v1/messages?beta=true
    ///
    /// Creates a non-streaming beta message.
    pub async fn create(&self, params: &BetaMessageCreateParams) -> Result<BetaMessage, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Creates a non-streaming beta message with per-request options.
    pub async fn create_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessage, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), true);
        self.client
            .validate_nonstreaming_timeout(&params.model, params.max_tokens, options)?;

        let mut body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!("failed to serialize BetaMessageCreateParams: {e}"))
        })?;
        transform_deprecated_output_format(&mut body)?;

        // Force stream to false for non-streaming create.
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::Value::Bool(false));
        }

        let extra_headers = beta_message_headers(
            params.betas.as_deref(),
            params.tools.as_deref(),
            &params.messages,
        );

        self.client
            .post_with_options(
                "/v1/messages?beta=true",
                &body,
                extra_headers.as_ref(),
                options,
            )
            .await
    }

    /// Rust equivalent of TS `client.beta.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Beta message creation returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), true);
        self.client
            .validate_nonstreaming_timeout(&params.model, params.max_tokens, options)?;

        let mut body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!("failed to serialize BetaMessageCreateParams: {e}"))
        })?;
        transform_deprecated_output_format(&mut body)?;

        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::Value::Bool(false));
        }

        let extra_headers = beta_message_headers(
            params.betas.as_deref(),
            params.tools.as_deref(),
            &params.messages,
        );

        self.client
            .post_with_response(
                "/v1/messages?beta=true",
                &body,
                extra_headers.as_ref(),
                options,
            )
            .await
    }

    /// Maps to: TS beta Messages.parse() -- create then parse structured output.
    ///
    /// Adds the `structured-outputs-2025-12-15` beta flag like the TS SDK and
    /// deserializes the first text block into caller-provided `T` using serde.
    pub async fn parse<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ParsedBetaMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_options(params, None).await
    }

    /// Structured-output beta parse with per-request options.
    pub async fn parse_with_options<T>(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ParsedBetaMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        Ok(self
            .parse_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Structured-output beta parse returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut params = params.clone();
        params
            .betas
            .get_or_insert_with(Vec::new)
            .push("structured-outputs-2025-12-15".to_owned());
        let should_parse = has_beta_json_schema_output_format(&params);
        let response = self
            .create_with_response_and_options(&params, options)
            .await?;
        let parsed = if should_parse {
            parse_beta_message(&response.data)?
        } else {
            parsed_beta_message_without_parsing(&response.data)
        };
        Ok(ApiResponse {
            data: parsed,
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to: TS Messages.create() with stream:true -- POST /v1/messages?beta=true
    ///
    /// Creates a streaming beta message and returns a raw SSE stream.
    pub async fn create_stream(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<SseStream<BetaMessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    /// Creates a streaming beta message with per-request options.
    pub async fn create_stream_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SseStream<BetaMessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.create({ stream: true }).withResponse()`.
    ///
    /// The raw response body remains owned by the returned SSE stream, so
    /// `response.body` is intentionally empty and only metadata/headers are
    /// captured.
    pub async fn create_stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Streaming beta creation with response metadata and per-request options.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        emit_message_create_warnings(&params.model, params.thinking.as_ref(), true);
        let mut body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!("failed to serialize BetaMessageCreateParams: {e}"))
        })?;
        transform_deprecated_output_format(&mut body)?;

        // Force stream to true.
        if let Some(obj) = body.as_object_mut() {
            obj.insert("stream".into(), serde_json::Value::Bool(true));
        }

        let extra_headers = beta_message_headers(
            params.betas.as_deref(),
            params.tools.as_deref(),
            &params.messages,
        );

        let response = self
            .client
            .post_stream_with_options(
                "/v1/messages?beta=true",
                &body,
                extra_headers.as_ref(),
                options,
            )
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: SseStream::new(response),
            response: raw,
            request_id,
        })
    }

    /// Maps to: TS beta Messages.stream() -- high-level streaming helper.
    ///
    /// Returns a [`BetaMessageStream`] that accumulates beta deltas into a
    /// final [`BetaMessage`].
    pub async fn stream(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<BetaMessageStream, ApiError> {
        self.stream_with_options(params, None).await
    }

    /// High-level beta streaming helper with per-request options.
    pub async fn stream_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageStream, ApiError> {
        Ok(self
            .stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `BetaMessageStream.withResponse()` for streams
    /// created from the resource helper.
    pub async fn stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// High-level beta streaming helper with response metadata and per-request options.
    pub async fn stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
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
            data: BetaMessageStream::new(Box::pin(response.data)),
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to: TS beta Messages.toolRunner().
    ///
    /// Creates a Rust beta tool runner bound to this client. The runner drives
    /// iterative beta message requests and appends beta tool-result blocks.
    pub fn tool_runner(&self, params: BetaToolRunnerParams) -> BetaToolRunner<'a> {
        BetaToolRunner::new(self.client, params)
    }

    /// TS-style camelCase alias for [`BetaMessages::tool_runner`].
    #[allow(non_snake_case)]
    pub fn toolRunner(&self, params: BetaToolRunnerParams) -> BetaToolRunner<'a> {
        self.tool_runner(params)
    }

    /// Maps to the optional request-options argument accepted by TS beta
    /// `Messages.toolRunner(params, options)`.
    pub fn tool_runner_with_options(
        &self,
        params: BetaToolRunnerParams,
        options: RequestOptions,
    ) -> BetaToolRunner<'a> {
        BetaToolRunner::new_with_options(self.client, params, options)
    }

    /// TS-style camelCase alias for [`BetaMessages::tool_runner_with_options`].
    #[allow(non_snake_case)]
    pub fn toolRunnerWithOptions(
        &self,
        params: BetaToolRunnerParams,
        options: RequestOptions,
    ) -> BetaToolRunner<'a> {
        self.tool_runner_with_options(params, options)
    }

    /// Maps to: TS Messages.countTokens() -- POST /v1/messages/count_tokens?beta=true
    ///
    /// Count the number of tokens in a beta message request.
    pub async fn count_tokens(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, None).await
    }

    /// TS-style camelCase alias for [`BetaMessages::count_tokens`].
    #[allow(non_snake_case)]
    pub async fn countTokens(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens(params).await
    }

    /// Count tokens in a beta message request with per-request options.
    pub async fn count_tokens_with_options(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        Ok(self
            .count_tokens_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count beta-message tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        let mut body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!(
                "failed to serialize BetaMessageCountTokensParams: {e}"
            ))
        })?;
        transform_deprecated_output_format(&mut body)?;

        let mut beta_values: Vec<String> = params.betas.clone().unwrap_or_default();
        beta_values.push("token-counting-2024-11-01".to_owned());

        let mut extra_headers = HashMap::new();
        extra_headers.insert("anthropic-beta".to_owned(), Some(beta_values.join(",")));

        self.client
            .post_with_response(
                "/v1/messages/count_tokens?beta=true",
                &body,
                Some(&extra_headers),
                options,
            )
            .await
    }

    /// TS-style camelCase alias for [`BetaMessages::count_tokens_with_options`].
    #[allow(non_snake_case)]
    pub async fn countTokensWithOptions(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, options).await
    }

    /// TS-style camelCase alias for [`BetaMessages::count_tokens_with_response`].
    #[allow(non_snake_case)]
    pub async fn countTokensWithResponse(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        self.count_tokens_with_response(params).await
    }
}

/// Maps to: TS beta messages `transformOutputFormat()`.
///
/// `output_format` is a deprecated beta request body field. The TS SDK accepts
/// it for compatibility, rejects it when `output_config.format` is also set,
/// and sends only `output_config.format` on the wire.
fn transform_deprecated_output_format(body: &mut serde_json::Value) -> Result<(), ApiError> {
    let Some(obj) = body.as_object_mut() else {
        return Err(ApiError::Sdk(
            "Beta message params did not serialize to object".to_owned(),
        ));
    };

    let Some(output_format) = obj.remove("output_format") else {
        return Ok(());
    };

    if output_format.is_null() {
        return Ok(());
    }

    let output_config = obj
        .entry("output_config".to_owned())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if output_config.is_null() {
        *output_config = serde_json::Value::Object(serde_json::Map::new());
    }

    let Some(output_config_obj) = output_config.as_object_mut() else {
        return Err(ApiError::Sdk(
            "output_config must be an object when output_format is provided".to_owned(),
        ));
    };

    if output_config_obj
        .get("format")
        .is_some_and(|format| !format.is_null())
    {
        return Err(ApiError::Sdk(
            "Both output_format and output_config.format were provided. Please use only output_config.format (output_format is deprecated).".to_owned(),
        ));
    }

    output_config_obj.insert("format".to_owned(), output_format);
    Ok(())
}

// Maps to: TS lib/tools/ToolError.ts
//
// Error type raised during tool execution. When a tool fails, the `message`
// is reported back to the model as the tool-result content (or `content` /
// `content_blocks` if explicitly set), and `is_error` is always `true`.

use crate::resources::beta::messages::types::BetaToolResultContentBlockParam;

/// Maps to: TS `ToolError` -- error raised during tool execution
///
/// When a tool fails, the `message` is reported back to the model as the
/// tool-result content (or `content` / `content_blocks` if explicitly set),
/// and `is_error` is always `true`.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct ToolError {
    /// Human-readable error description.
    pub message: String,
    /// Optional text override for the content sent back to the model.
    /// When both `content` and `content_blocks` are `None`, `message` is used.
    pub content: Option<String>,
    /// Optional structured beta tool-result content blocks.
    ///
    /// Maps to TS `new ToolError(Array<BetaToolResultContentBlockParam>)`.
    /// Stable tool-runner paths serialize these blocks as JSON values; beta
    /// tool-runner paths preserve the typed blocks.
    pub content_blocks: Option<Vec<BetaToolResultContentBlockParam>>,
    /// Always `true` for errors; kept for parity with the TS SDK.
    pub is_error: bool,
}

impl ToolError {
    /// Create a new `ToolError` with the given message.
    /// `is_error` defaults to `true` and `content` to `None`.
    ///
    /// Maps to: TS `new ToolError(content)`
    pub fn new(message: impl Into<String>) -> Self {
        ToolError {
            message: message.into(),
            content: None,
            content_blocks: None,
            is_error: true,
        }
    }

    /// Create a `ToolError` with explicit text content override.
    ///
    /// Maps to: TS `new ToolError(content)` with string content.
    pub fn with_content(message: impl Into<String>, content: impl Into<String>) -> Self {
        ToolError {
            message: message.into(),
            content: Some(content.into()),
            content_blocks: None,
            is_error: true,
        }
    }

    /// Create a `ToolError` with structured beta tool-result content blocks.
    ///
    /// Maps to: TS `new ToolError(contentBlocks)`.
    pub fn with_content_blocks(content_blocks: Vec<BetaToolResultContentBlockParam>) -> Self {
        let message = message_from_content_blocks(&content_blocks);
        ToolError {
            message,
            content: None,
            content_blocks: Some(content_blocks),
            is_error: true,
        }
    }
}

fn message_from_content_blocks(blocks: &[BetaToolResultContentBlockParam]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            BetaToolResultContentBlockParam::Text(text) => text.text.clone(),
            BetaToolResultContentBlockParam::Image(_) => "[image]".to_owned(),
            BetaToolResultContentBlockParam::SearchResult(_) => "[search_result]".to_owned(),
            BetaToolResultContentBlockParam::Document(_) => "[document]".to_owned(),
            BetaToolResultContentBlockParam::ToolReference(_) => "[tool_reference]".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_error_new_sets_defaults() {
        let err = ToolError::new("something went wrong");
        assert_eq!(err.message, "something went wrong");
        assert!(err.content.is_none());
        assert!(err.content_blocks.is_none());
        assert!(err.is_error);
    }

    #[test]
    fn tool_error_with_content_overrides() {
        let err = ToolError::with_content("internal failure", "safe message for model");
        assert_eq!(err.message, "internal failure");
        assert_eq!(err.content.as_deref(), Some("safe message for model"));
        assert!(err.content_blocks.is_none());
        assert!(err.is_error);
    }

    #[test]
    fn tool_error_with_content_blocks_builds_ts_like_message() {
        use crate::resources::beta::messages::types::BetaTextBlockParam;

        let err = ToolError::with_content_blocks(vec![BetaToolResultContentBlockParam::Text(
            BetaTextBlockParam {
                text: "details".to_owned(),
                stainless_helpers: Vec::new(),
                cache_control: None,
                citations: None,
            },
        )]);
        assert_eq!(err.message, "details");
        assert!(err.content.is_none());
        assert_eq!(err.content_blocks.as_ref().unwrap().len(), 1);
        assert!(err.is_error);
    }

    #[test]
    fn tool_error_display() {
        let err = ToolError::new("bad input");
        let display = format!("{err}");
        assert_eq!(display, "bad input");
    }
}

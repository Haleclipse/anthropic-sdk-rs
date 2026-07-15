// Maps to: TS lib/tools/CompactionControl.ts
//
// Configuration used by higher-level tool-runner helpers to decide when and
// how to summarize accumulated conversation history.

use serde::{Deserialize, Serialize};

/// Maps to TS `DEFAULT_TOKEN_THRESHOLD`.
pub const DEFAULT_TOKEN_THRESHOLD: u64 = 100_000;

/// Maps to TS `DEFAULT_SUMMARY_PROMPT`.
pub const DEFAULT_SUMMARY_PROMPT: &str = "You have been working on the task described above but have not yet completed it. Write a continuation summary that will allow you (or another instance of yourself) to resume work efficiently in a future context window where the conversation history will be replaced with this summary. Your summary should be structured, concise, and actionable. Include:\n1. Task Overview\nThe user's core request and success criteria\nAny clarifications or constraints they specified\n2. Current State\nWhat has been completed so far\nFiles created, modified, or analyzed (with paths if relevant)\nKey outputs or artifacts produced\n3. Important Discoveries\nTechnical constraints or requirements uncovered\nDecisions made and their rationale\nErrors encountered and how they were resolved\nWhat approaches were tried that didn't work (and why)\n4. Next Steps\nSpecific actions needed to complete the task\nAny blockers or open questions to resolve\nPriority order if multiple steps remain\n5. Context to Preserve\nUser preferences or style requirements\nDomain-specific details that aren't obvious\nAny promises made to the user\nBe concise but complete—err on the side of including information that would prevent duplicate work or repeated mistakes. Write in a way that enables immediate resumption of the task.\nWrap your summary in <summary></summary> tags.";

/// Maps to TS `CompactionControl`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionControl {
    /// Whether automatic compaction is enabled.
    pub enabled: bool,

    /// Context token threshold at which compaction should trigger.
    #[serde(
        rename = "contextTokenThreshold",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_token_threshold: Option<u64>,

    /// Model to use for generating compaction summaries. When absent, callers
    /// should default to the main tool-runner model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Prompt used to instruct the model on summary generation.
    #[serde(rename = "summaryPrompt", skip_serializing_if = "Option::is_none")]
    pub summary_prompt: Option<String>,
}

impl CompactionControl {
    /// Create a compaction-control config with TS defaults filled in when
    /// `enabled` is true.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            context_token_threshold: Some(DEFAULT_TOKEN_THRESHOLD),
            model: None,
            summary_prompt: Some(DEFAULT_SUMMARY_PROMPT.to_owned()),
        }
    }

    /// Create a disabled compaction-control config.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            context_token_threshold: None,
            model: None,
            summary_prompt: None,
        }
    }
}

impl Default for CompactionControl {
    fn default() -> Self {
        Self::disabled()
    }
}

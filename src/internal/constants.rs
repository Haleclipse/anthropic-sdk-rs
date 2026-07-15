// Maps to: TS internal/constants.ts
//
//! Shared constants for the Anthropic Rust SDK.
//!
//! Centralised definitions for default configuration values and model-specific
//! constraints, mirroring the TypeScript SDK's `internal/constants.ts`.

use std::collections::HashMap;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// API configuration defaults
// ---------------------------------------------------------------------------

/// Maps to: TS `anthropic-version` header value used throughout the SDK.
///
/// The Anthropic API version string sent with every request.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maps to: TS `BaseAnthropic.DEFAULT_TIMEOUT` / default timeout.
///
/// Default request timeout in milliseconds (10 minutes).
pub const DEFAULT_TIMEOUT_MS: u64 = 600_000;

/// Maps to: TS default `maxRetries` value.
///
/// Default number of automatic retries for transient failures.
pub const DEFAULT_MAX_RETRIES: u32 = 2;

/// Maps to: TS default base URL `https://api.anthropic.com`.
///
/// The default API endpoint used when no override is provided.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

// ---------------------------------------------------------------------------
// Model-specific non-streaming token limits
// ---------------------------------------------------------------------------

/// Maps to: TS `MODEL_NONSTREAMING_TOKENS` in internal/constants.ts
///
/// Lazily-initialised map of model identifiers to their maximum non-streaming
/// output token count.  Used to apply model-specific timeout constraints.
static MODEL_NONSTREAMING_TOKENS: OnceLock<HashMap<&'static str, u64>> = OnceLock::new();

/// Returns the lazily-initialised model token map.
fn get_model_tokens() -> &'static HashMap<&'static str, u64> {
    MODEL_NONSTREAMING_TOKENS.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("claude-opus-4-20250514", 8192);
        m.insert("claude-opus-4-0", 8192);
        m.insert("claude-4-opus-20250514", 8192);
        m.insert("anthropic.claude-opus-4-20250514-v1:0", 8192);
        m.insert("claude-opus-4@20250514", 8192);
        m.insert("claude-opus-4-1-20250805", 8192);
        m.insert("anthropic.claude-opus-4-1-20250805-v1:0", 8192);
        m.insert("claude-opus-4-1@20250805", 8192);
        m
    })
}

/// Maps to: TS `MODEL_NONSTREAMING_TOKENS` lookup in internal/constants.ts
///
/// Returns the non-streaming output-token cap for the given model identifier,
/// or `None` if the model is not in the known list.
pub fn model_nonstreaming_tokens(model: &str) -> Option<u64> {
    get_model_tokens().get(model).copied()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_have_expected_values() {
        assert_eq!(ANTHROPIC_VERSION, "2023-06-01");
        assert_eq!(DEFAULT_TIMEOUT_MS, 600_000);
        assert_eq!(DEFAULT_MAX_RETRIES, 2);
        assert_eq!(DEFAULT_BASE_URL, "https://api.anthropic.com");
    }

    #[test]
    fn model_nonstreaming_tokens_known_model() {
        assert_eq!(
            model_nonstreaming_tokens("claude-opus-4-20250514"),
            Some(8192)
        );
        assert_eq!(model_nonstreaming_tokens("claude-opus-4-0"), Some(8192));
        assert_eq!(
            model_nonstreaming_tokens("claude-4-opus-20250514"),
            Some(8192)
        );
        assert_eq!(
            model_nonstreaming_tokens("anthropic.claude-opus-4-20250514-v1:0"),
            Some(8192)
        );
        assert_eq!(
            model_nonstreaming_tokens("claude-opus-4@20250514"),
            Some(8192)
        );
        assert_eq!(
            model_nonstreaming_tokens("claude-opus-4-1-20250805"),
            Some(8192)
        );
        assert_eq!(
            model_nonstreaming_tokens("anthropic.claude-opus-4-1-20250805-v1:0"),
            Some(8192)
        );
        assert_eq!(
            model_nonstreaming_tokens("claude-opus-4-1@20250805"),
            Some(8192)
        );
    }

    #[test]
    fn model_nonstreaming_tokens_unknown_model() {
        assert_eq!(model_nonstreaming_tokens("claude-3-sonnet"), None);
        assert_eq!(model_nonstreaming_tokens("gpt-4"), None);
        assert_eq!(model_nonstreaming_tokens(""), None);
    }
}

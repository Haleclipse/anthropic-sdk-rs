// Maps to: TS resources/beta/beta.ts

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;

use super::files::Files;
use super::messages::BetaMessages;
use super::models::BetaModels;
use super::skills::Skills;

// ─────────────────────────────────────────────────────────────────────────────
// AnthropicBeta type
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS AnthropicBeta
///
/// A beta feature identifier string sent via the `anthropic-beta` header.
/// Well-known values are provided as associated constants.
pub type AnthropicBeta = String;

/// Well-known beta feature identifiers.
pub mod beta_features {
    pub const MESSAGE_BATCHES: &str = "message-batches-2024-09-24";
    pub const PROMPT_CACHING: &str = "prompt-caching-2024-07-31";
    pub const COMPUTER_USE_2024: &str = "computer-use-2024-10-22";
    pub const COMPUTER_USE_2025: &str = "computer-use-2025-01-24";
    pub const PDFS: &str = "pdfs-2024-09-25";
    pub const TOKEN_COUNTING: &str = "token-counting-2024-11-01";
    pub const TOKEN_EFFICIENT_TOOLS: &str = "token-efficient-tools-2025-02-19";
    pub const OUTPUT_128K: &str = "output-128k-2025-02-19";
    pub const FILES_API: &str = "files-api-2025-04-14";
    pub const MCP_CLIENT: &str = "mcp-client-2025-04-04";
    pub const MCP_CLIENT_V2: &str = "mcp-client-2025-11-20";
    pub const DEV_FULL_THINKING: &str = "dev-full-thinking-2025-05-14";
    pub const INTERLEAVED_THINKING: &str = "interleaved-thinking-2025-05-14";
    pub const CODE_EXECUTION: &str = "code-execution-2025-05-22";
    pub const EXTENDED_CACHE_TTL: &str = "extended-cache-ttl-2025-04-11";
    pub const CONTEXT_1M: &str = "context-1m-2025-08-07";
    pub const CONTEXT_MANAGEMENT: &str = "context-management-2025-06-27";
    pub const MODEL_CONTEXT_WINDOW_EXCEEDED: &str = "model-context-window-exceeded-2025-08-26";
    pub const SKILLS: &str = "skills-2025-10-02";
    pub const FAST_MODE: &str = "fast-mode-2026-02-01";
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta error types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaError
///
/// Union of all beta-specific error types returned in `BetaErrorResponse`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaError {
    /// Maps to: TS BetaInvalidRequestError
    #[serde(rename = "invalid_request_error")]
    InvalidRequest { message: String },

    /// Maps to: TS BetaAuthenticationError
    #[serde(rename = "authentication_error")]
    Authentication { message: String },

    /// Maps to: TS BetaBillingError
    #[serde(rename = "billing_error")]
    Billing { message: String },

    /// Maps to: TS BetaPermissionError
    #[serde(rename = "permission_error")]
    Permission { message: String },

    /// Maps to: TS BetaNotFoundError
    #[serde(rename = "not_found_error")]
    NotFound { message: String },

    /// Maps to: TS BetaRateLimitError
    #[serde(rename = "rate_limit_error")]
    RateLimit { message: String },

    /// Maps to: TS BetaGatewayTimeoutError
    #[serde(rename = "timeout_error")]
    GatewayTimeout { message: String },

    /// Maps to: TS BetaAPIError
    #[serde(rename = "api_error")]
    Api { message: String },

    /// Maps to: TS BetaOverloadedError
    #[serde(rename = "overloaded_error")]
    Overloaded { message: String },
}

/// TS export-name compatibility alias for `BetaAPIError`.
///
/// Rust models the TS beta error-object union as [`BetaError`] enum variants;
/// these aliases preserve the TS interface names in the beta resource namespace.
pub type BetaAPIError = BetaError;
/// TS export-name compatibility alias for `BetaAuthenticationError`.
pub type BetaAuthenticationError = BetaError;
/// TS export-name compatibility alias for `BetaBillingError`.
pub type BetaBillingError = BetaError;
/// TS export-name compatibility alias for `BetaGatewayTimeoutError`.
pub type BetaGatewayTimeoutError = BetaError;
/// TS export-name compatibility alias for `BetaInvalidRequestError`.
pub type BetaInvalidRequestError = BetaError;
/// TS export-name compatibility alias for `BetaNotFoundError`.
pub type BetaNotFoundError = BetaError;
/// TS export-name compatibility alias for `BetaOverloadedError`.
pub type BetaOverloadedError = BetaError;
/// TS export-name compatibility alias for `BetaPermissionError`.
pub type BetaPermissionError = BetaError;
/// TS export-name compatibility alias for `BetaRateLimitError`.
pub type BetaRateLimitError = BetaError;

impl BetaError {
    /// Returns the error message regardless of variant.
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidRequest { message }
            | Self::Authentication { message }
            | Self::Billing { message }
            | Self::Permission { message }
            | Self::NotFound { message }
            | Self::RateLimit { message }
            | Self::GatewayTimeout { message }
            | Self::Api { message }
            | Self::Overloaded { message } => message,
        }
    }
}

/// Maps to: TS BetaErrorResponse
///
/// The top-level error envelope returned by beta API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaErrorResponse {
    /// The typed error body.
    pub error: BetaError,

    /// The server-assigned request ID, if available.
    pub request_id: Option<String>,

    /// Object type. Always `"error"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaErrorResponse {
    fn default() -> Self {
        Self {
            error: BetaError::Api {
                message: String::new(),
            },
            request_id: None,
            type_name: "error".to_owned(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Beta resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS Beta class
///
/// Top-level namespace for beta API resources. Provides access to beta
/// messages, models, files, and skills sub-resources.
///
/// Obtain an instance via [`Anthropic::beta`]:
///
/// ```ignore
/// let client = Anthropic::new(ClientOptions::default())?;
/// let msg = client.beta().messages().create(&params).await?;
/// ```
pub struct Beta<'a> {
    client: &'a Anthropic,
}

impl<'a> Beta<'a> {
    /// Create a new `Beta` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Access the beta Messages sub-resource.
    pub fn messages(&self) -> BetaMessages<'a> {
        BetaMessages::new(self.client)
    }

    /// Access the beta Models sub-resource.
    pub fn models(&self) -> BetaModels<'a> {
        BetaModels::new(self.client)
    }

    /// Access the beta Files sub-resource.
    pub fn files(&self) -> Files<'a> {
        Files::new(self.client)
    }

    /// Access the beta Skills sub-resource.
    pub fn skills(&self) -> Skills<'a> {
        Skills::new(self.client)
    }
}

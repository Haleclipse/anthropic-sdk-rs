// Maps to: TS resources/shared.ts

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Error object variants
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS ErrorObject
///
/// The typed error body returned in Anthropic API error responses.
/// Each variant carries a human-readable `message` and a fixed `type` tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ErrorObject {
    /// Maps to: TS InvalidRequestError
    #[serde(rename = "invalid_request_error")]
    InvalidRequest { message: String },

    /// Maps to: TS AuthenticationError
    #[serde(rename = "authentication_error")]
    Authentication { message: String },

    /// Maps to: TS BillingError
    #[serde(rename = "billing_error")]
    Billing { message: String },

    /// Maps to: TS PermissionError
    #[serde(rename = "permission_error")]
    Permission { message: String },

    /// Maps to: TS NotFoundError
    #[serde(rename = "not_found_error")]
    NotFound { message: String },

    /// Maps to: TS RateLimitError
    #[serde(rename = "rate_limit_error")]
    RateLimit { message: String },

    /// Maps to: TS GatewayTimeoutError
    #[serde(rename = "timeout_error")]
    GatewayTimeout { message: String },

    /// Maps to: TS APIErrorObject
    #[serde(rename = "api_error")]
    Api { message: String },

    /// Maps to: TS OverloadedError
    #[serde(rename = "overloaded_error")]
    Overloaded { message: String },
}

/// TS export-name compatibility alias for `APIErrorObject`.
///
/// Rust models the TS shared error-object union as [`ErrorObject`] enum
/// variants, so these aliases preserve the TS interface names while keeping
/// pattern matching centralized.
pub type APIErrorObject = ErrorObject;
/// TS export-name compatibility alias for `AuthenticationError`.
pub type AuthenticationError = ErrorObject;
/// TS export-name compatibility alias for `BillingError`.
pub type BillingError = ErrorObject;
/// TS export-name compatibility alias for `GatewayTimeoutError`.
pub type GatewayTimeoutError = ErrorObject;
/// TS export-name compatibility alias for `InvalidRequestError`.
pub type InvalidRequestError = ErrorObject;
/// TS export-name compatibility alias for `NotFoundError`.
pub type NotFoundError = ErrorObject;
/// TS export-name compatibility alias for `OverloadedError`.
pub type OverloadedError = ErrorObject;
/// TS export-name compatibility alias for `PermissionError`.
pub type PermissionError = ErrorObject;
/// TS export-name compatibility alias for `RateLimitError`.
pub type RateLimitError = ErrorObject;

impl ErrorObject {
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

    /// Returns the type discriminator string (e.g. `"invalid_request_error"`).
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::InvalidRequest { .. } => "invalid_request_error",
            Self::Authentication { .. } => "authentication_error",
            Self::Billing { .. } => "billing_error",
            Self::Permission { .. } => "permission_error",
            Self::NotFound { .. } => "not_found_error",
            Self::RateLimit { .. } => "rate_limit_error",
            Self::GatewayTimeout { .. } => "timeout_error",
            Self::Api { .. } => "api_error",
            Self::Overloaded { .. } => "overloaded_error",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ErrorResponse
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS ErrorResponse
///
/// The top-level error envelope returned by the Anthropic API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// The typed error body.
    pub error: ErrorObject,

    /// The server-assigned request ID, if available.
    pub request_id: Option<String>,

    /// Object type. Always `"error"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for ErrorResponse {
    fn default() -> Self {
        Self {
            error: ErrorObject::Api {
                message: String::new(),
            },
            request_id: None,
            type_name: "error".to_owned(),
        }
    }
}

//! Maps to: `@azure/msal-common` 16.4.0
//! `src/response/ServerAuthorizationTokenResponse.ts`, for the fields
//! `ResponseHandler` reads.

use serde_json::Value;

/// The token endpoint's response: the parsed body with the status
/// (`ClientCredentialClient.ts:316-317`), or what a managed identity source
/// makes of its endpoint's (`BaseManagedIdentitySource.getServerTokenResponse`).
/// The fields hold whatever JSON was sent, read with JS semantics.
pub(crate) struct ServerAuthorizationTokenResponse {
    pub status: u16,
    pub token_type: Option<Value>,
    pub scope: Option<Value>,
    /// A number, as `generateCacheRecord` reads it
    /// (`ResponseHandler.ts:417-428`): a JSON body's value goes through
    /// [`super::response_handler::js_seconds`].
    pub expires_in: Option<f64>,
    /// As `expires_in`.
    pub refresh_in: Option<f64>,
    pub access_token: Option<Value>,
    pub error: Option<Value>,
    pub error_description: Option<Value>,
    pub error_codes: Option<Value>,
    pub suberror: Option<Value>,
    pub timestamp: Option<Value>,
    pub trace_id: Option<Value>,
    pub correlation_id: Option<Value>,
}

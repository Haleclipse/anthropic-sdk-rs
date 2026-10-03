//! Maps to: `@azure/msal-common` 16.4.0 `src/response/AuthenticationResult.ts`,
//! for the fields the identity layer reads.

/// The dates are `getTime()` values (`NaN` for an invalid date); `expires_on`
/// is the `null` of a response without an access token.
pub(crate) struct AuthenticationResult {
    pub access_token: String,
    pub expires_on: Option<f64>,
    pub refresh_on: Option<f64>,
}

//! Maps to: `@azure/msal-common` 16.4.0 `src/utils/Constants.ts`, for the
//! constants the ported MSAL code reads.

/// `:26` `OFFLINE_ACCESS_SCOPE`.
pub(crate) const OFFLINE_ACCESS_SCOPE: &str = "offline_access";

/// `:77-83`: `OIDC_DEFAULT_SCOPES`, and `OIDC_SCOPES`, which adds `email`.
pub(crate) const OIDC_DEFAULT_SCOPES: [&str; 3] = ["openid", "profile", "offline_access"];
pub(crate) const OIDC_SCOPES: [&str; 4] = ["openid", "profile", "offline_access", "email"];

/// `:112-116` `AADAuthority`.
pub(crate) const AAD_AUTHORITIES: [&str; 3] = ["common", "organizations", "consumers"];

/// `:41-47`: `KNOWN_PUBLIC_CLOUDS` and the host their regional
/// endpoints move to.
pub(crate) const KNOWN_PUBLIC_CLOUDS: [&str; 4] = [
    "login.microsoftonline.com",
    "login.windows.net",
    "login.microsoft.com",
    "sts.windows.net",
];
pub(crate) const REGIONAL_AUTH_PUBLIC_CLOUD_SUFFIX: &str = "login.microsoft.com";

/// `:30-31` `URL_FORM_CONTENT_TYPE`.
pub(crate) const URL_FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded;charset=utf-8";

/// `:34` `NOT_AVAILABLE`.
pub(crate) const NOT_AVAILABLE: &str = "Not Available";

/// `:349` `DEFAULT_TOKEN_RENEWAL_OFFSET_SEC`: a token within five minutes of
/// expiry is treated as expired.
pub(crate) const DEFAULT_TOKEN_RENEWAL_OFFSET_SEC: f64 = 300.0;

/// `:324-336` `CacheOutcome`, for the outcomes
/// `ClientCredentialClient.getCachedAuthenticationResult` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CacheOutcome {
    NotApplicable,
    NoCachedAccessToken,
    CachedAccessTokenExpired,
    ProactivelyRefreshed,
}

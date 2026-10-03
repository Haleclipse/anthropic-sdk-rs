//! Maps to: `@azure/identity` `dist/esm/constants.js`, for the constant both
//! the identity client (`client/identityClient.js:8`) and the MSAL
//! configuration (`msal/utils.js:5`) import.

/// `constants.js:43,49` `DefaultAuthorityHost`
/// (`AzureAuthorityHosts.AzurePublicCloud`).
pub(crate) const DEFAULT_AUTHORITY_HOST: &str = "https://login.microsoftonline.com";

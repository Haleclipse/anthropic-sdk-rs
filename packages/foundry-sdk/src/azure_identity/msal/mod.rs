//! Maps to: the parts of `@azure/msal-common` 16.4.0 (and, for
//! `client_credential_client`, `@azure/msal-node` 5.1.1) that both MSAL flows
//! run through: `ClientSecretCredential`'s `ConfidentialClientApplication`
//! and `ManagedIdentityCredential`'s `ManagedIdentityApplication`. Each file is
//! named after the MSAL source file it maps to.
//!
//! What they share is how a token response is validated and cached
//! (`ResponseHandler`), how the cache is read and written (`ClientCredentialClient`
//! `getCachedAuthenticationResult`, `CacheManager`, `ScopeSet`), the errors
//! MSAL throws, and its time and URL helpers. Where the tokens are kept is each
//! flow's own: a client secret credential's storage lives as long as the
//! credential, the managed identity storage is static. What only the
//! client-credential flow runs (authority resolution, throttling, the token
//! request) stays in `credentials/client_secret_credential.rs`.

pub(crate) mod access_token_entity;
pub(crate) mod auth_error;
pub(crate) mod authentication_result;
pub(crate) mod cache_manager;
pub(crate) mod client_credential_client;
pub(crate) mod constants;
pub(crate) mod interaction_required_auth_error;
pub(crate) mod network_response;
pub(crate) mod response_handler;
pub(crate) mod scope_set;
pub(crate) mod server_authorization_token_response;
pub(crate) mod time_utils;
pub(crate) mod url_string;
pub(crate) mod url_utils;

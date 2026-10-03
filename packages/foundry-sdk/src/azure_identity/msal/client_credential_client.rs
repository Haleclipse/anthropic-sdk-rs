//! Maps to: `@azure/msal-node` 5.1.1 `src/client/ClientCredentialClient.ts`,
//! for the cache read both flows run: `ManagedIdentityApplication` borrows it
//! through a client of its own (`ManagedIdentityApplication.ts:163-171`).
//! The rest of the client-credential flow is in
//! `credentials/client_secret_credential.rs`.

use super::access_token_entity::AccessTokenEntity;
use super::auth_error::MsalError;
use super::authentication_result::AuthenticationResult;
use super::cache_manager::get_access_tokens_by_filter;
use super::constants::{CacheOutcome, DEFAULT_TOKEN_RENEWAL_OFFSET_SEC};
use super::response_handler::generate_authentication_result;
use super::scope_set::{create_search_scopes, scope_set};
use super::time_utils::is_token_expired;

/// Maps to: `ClientCredentialClient.ts:106-205`
/// `getCachedAuthenticationResult`, without the persistence plugin and
/// telemetry. `access_tokens` is the storage's, `id` the client id or the
/// managed identity's. A missing or expired token is `None`; a token past its
/// refresh time comes with `ProactivelyRefreshed`.
pub(crate) fn get_cached_authentication_result(
    access_tokens: &[AccessTokenEntity],
    id: &str,
    request_scopes: &[String],
) -> Result<(Option<AuthenticationResult>, CacheOutcome), MsalError> {
    let Some(cached_access_token) =
        read_access_token_from_cache(access_tokens, id, &scope_set(request_scopes)?)?
    else {
        return Ok((None, CacheOutcome::NoCachedAccessToken));
    };
    if is_token_expired(
        cached_access_token.expires_on,
        DEFAULT_TOKEN_RENEWAL_OFFSET_SEC,
    ) {
        return Ok((None, CacheOutcome::CachedAccessTokenExpired));
    }
    let last_cache_outcome = if cached_access_token
        .refresh_on
        .is_some_and(|refresh_on| is_token_expired(refresh_on, 0.0))
    {
        CacheOutcome::ProactivelyRefreshed
    } else {
        CacheOutcome::NotApplicable
    };
    Ok((
        Some(generate_authentication_result(cached_access_token)),
        last_cache_outcome,
    ))
}

/// Maps to: `ClientCredentialClient.ts:210-239` `readAccessTokenFromCache`:
/// the one token whose scopes hold the search scopes; more than one is
/// `multiple_matching_tokens`.
fn read_access_token_from_cache<'a>(
    access_tokens: &'a [AccessTokenEntity],
    id: &str,
    scope_set: &[String],
) -> Result<Option<&'a AccessTokenEntity>, MsalError> {
    let target = create_search_scopes(scope_set)?;
    match get_access_tokens_by_filter(access_tokens, id, &target).as_slice() {
        [] => Ok(None),
        [access_token] => Ok(Some(*access_token)),
        _ => Err(MsalError::client_auth("multiple_matching_tokens")),
    }
}

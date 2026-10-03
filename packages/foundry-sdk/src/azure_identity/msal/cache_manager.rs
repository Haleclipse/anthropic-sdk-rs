//! Maps to: `@azure/msal-common` 16.4.0 `src/cache/CacheManager.ts`, for the
//! access tokens. Where they are kept is the caller's (a client secret
//! credential's own `NodeStorage`, or `ManagedIdentityApplication`'s static
//! one); these are the rules both are read and written by.

use super::access_token_entity::AccessTokenEntity;
use super::scope_set::{
    contains_only_oidc_scopes, contains_scope_set, intersecting_scope_sets, remove_oidc_scopes,
    scope_set_from_string,
};

/// Maps to: `CacheManager.ts:634-678` `saveAccessToken`: the entries of the
/// same client whose scopes intersect the new token's are replaced by it.
/// The filter's other fields are the same for every entry of a storage.
pub(crate) fn save_access_token(
    access_tokens: &mut Vec<AccessTokenEntity>,
    credential: AccessTokenEntity,
) {
    let Ok(mut current_scopes) = scope_set_from_string(&credential.target) else {
        return;
    };
    // `intersectingScopeSets` strips the OIDC scopes from its argument
    // unless they are all it holds.
    if !contains_only_oidc_scopes(&current_scopes) {
        remove_oidc_scopes(&mut current_scopes);
    }
    access_tokens.retain(|token_entity| {
        token_entity.client_id != credential.client_id
            || !scope_set_from_string(&token_entity.target).is_ok_and(|token_scope_set| {
                intersecting_scope_sets(&token_scope_set, &current_scopes)
            })
    });
    access_tokens.push(credential);
}

/// Maps to: `CacheManager.ts:1407-1432` `getAccessTokensByFilter`, for the
/// filter `readAccessTokenFromCache` builds (`ClientCredentialClient.ts:217-225`):
/// the client id (`matchClientId`, `:1731-1736`; never empty here) and the
/// target (`matchTarget`, `:1830-1843`). The other fields are the same for
/// every entry of a storage.
pub(crate) fn get_access_tokens_by_filter<'a>(
    access_tokens: &'a [AccessTokenEntity],
    client_id: &str,
    target: &[String],
) -> Vec<&'a AccessTokenEntity> {
    access_tokens
        .iter()
        .filter(|access_token| {
            access_token.client_id == client_id
                && scope_set_from_string(&access_token.target)
                    .is_ok_and(|entity_scope_set| contains_scope_set(&entity_scope_set, target))
        })
        .collect()
}

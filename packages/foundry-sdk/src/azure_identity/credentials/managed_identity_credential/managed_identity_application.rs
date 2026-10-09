//! Maps to: `@azure/msal-node` `src/client/ManagedIdentityApplication.ts`,
//! with the access-token half of its static `nodeStorage`.
//!
//! The cache is static (`:52`, `:80-87`), so every credential in the process
//! shares it. It is read and written by the rules the client-credential flow
//! uses too (`ClientCredentialClient.getCachedAuthenticationResult`, which
//! `:163-171` borrows, and msal-common's `CacheManager` and `ScopeSet`), in
//! [`crate::azure_identity::msal`]: a token is found by the identity's
//! id (the client id, or `system_assigned_managed_identity`) and by scope,
//! case-insensitively; the authority, realm and token type MSAL also matches
//! on are the same for every managed identity token.

use std::sync::Mutex;

use super::base_managed_identity_source::ManagedIdentityRequest;
use super::managed_identity_client::{self, ManagedIdentitySourceNames};
use super::{IdentityClient, MsiError};
use crate::azure_identity::Environment;
use crate::azure_identity::msal::access_token_entity::AccessTokenEntity;
use crate::azure_identity::msal::authentication_result::AuthenticationResult;
use crate::azure_identity::msal::cache_manager;
use crate::azure_identity::msal::client_credential_client::get_cached_authentication_result;
use crate::azure_identity::msal::constants::CacheOutcome;

/// Maps to: `src/utils/Constants.ts` `DEFAULT_MANAGED_IDENTITY_ID`.
const DEFAULT_MANAGED_IDENTITY_ID: &str = "system_assigned_managed_identity";

/// Maps to: `ManagedIdentityIdType`, for the identities CC can name: a
/// client id or none (resource and object ids are never passed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManagedIdentityIdType {
    SystemAssigned,
    UserAssignedClientId,
}

/// Maps to: `src/config/ManagedIdentityId.ts:33-71` `ManagedIdentityId`.
pub(super) struct ManagedIdentityId {
    pub id: String,
    pub id_type: ManagedIdentityIdType,
}

impl ManagedIdentityId {
    fn new(user_assigned_client_id: Option<&str>) -> Self {
        match user_assigned_client_id.filter(|id| !id.is_empty()) {
            Some(id) => Self {
                id: id.to_owned(),
                id_type: ManagedIdentityIdType::UserAssignedClientId,
            },
            None => Self {
                id: DEFAULT_MANAGED_IDENTITY_ID.to_owned(),
                id_type: ManagedIdentityIdType::SystemAssigned,
            },
        }
    }
}

/// `ManagedIdentityApplication.nodeStorage`'s access tokens.
static NODE_STORAGE: Mutex<Vec<AccessTokenEntity>> = Mutex::new(Vec::new());

/// Maps to: `ManagedIdentityApplication`.
pub(super) struct ManagedIdentityApplication {
    managed_identity_id: ManagedIdentityId,
    /// The `process.env` MSAL detects the source from.
    env: Environment,
}

impl ManagedIdentityApplication {
    /// Maps to: `:66-126` the constructor, for what it keeps: the identity.
    pub(super) fn new(env: Environment, user_assigned_client_id: Option<&str>) -> Self {
        Self {
            managed_identity_id: ManagedIdentityId::new(user_assigned_client_id),
            env,
        }
    }

    /// Maps to: `:133-233` `acquireToken`, without `forceRefresh` and claims,
    /// which CC never passes. `resource` is non-empty (the credential checked),
    /// so the `urlEmptyError` check (`:136-140`) cannot fire.
    ///
    /// A cached token past its refresh time is refreshed first, and the call
    /// waits for that refresh but returns the cached token (`:204-225`); a
    /// refresh that throws fails the call.
    pub(super) async fn acquire_token(
        &self,
        resource: &str,
        network_client: &IdentityClient<'_>,
    ) -> Result<AuthenticationResult, MsiError> {
        // `:142-155`: `resource.replace("/.default", "")`, the first one,
        // which is also the request's one scope.
        let managed_identity_request = ManagedIdentityRequest {
            resource: resource.replacen("/.default", "", 1),
        };
        // `:163-171`: the cache read of `ClientCredentialClient`, on the
        // static storage.
        let (cached_authentication_result, last_cache_outcome) = get_cached_authentication_result(
            &NODE_STORAGE.lock().unwrap(),
            &self.managed_identity_id.id,
            std::slice::from_ref(&managed_identity_request.resource),
        )?;
        match cached_authentication_result {
            Some(cached) => {
                if last_cache_outcome == CacheOutcome::ProactivelyRefreshed {
                    managed_identity_client::send_managed_identity_token_request(
                        &self.env,
                        &managed_identity_request,
                        &self.managed_identity_id,
                        true,
                        network_client,
                    )
                    .await?;
                }
                Ok(cached)
            }
            None => {
                managed_identity_client::send_managed_identity_token_request(
                    &self.env,
                    &managed_identity_request,
                    &self.managed_identity_id,
                    false,
                    network_client,
                )
                .await
            }
        }
    }

    /// Maps to: `:263-268` `getManagedIdentitySource`: the name detected
    /// first in this process.
    pub(super) fn get_managed_identity_source(&self) -> ManagedIdentitySourceNames {
        managed_identity_client::source_name()
            .unwrap_or_else(|| managed_identity_client::get_managed_identity_source(&self.env))
    }
}

/// Maps to: msal-common `CacheManager.ts:634-678` `saveAccessToken`, on the
/// static `nodeStorage`: the identity's tokens with an intersecting scope are
/// replaced.
pub(super) fn save_access_token(credential: AccessTokenEntity) {
    cache_manager::save_access_token(&mut NODE_STORAGE.lock().unwrap(), credential);
}

#[cfg(test)]
pub(super) fn reset() {
    NODE_STORAGE.lock().unwrap().clear();
}

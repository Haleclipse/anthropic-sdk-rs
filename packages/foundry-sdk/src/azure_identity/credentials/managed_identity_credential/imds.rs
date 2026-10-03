//! Maps to: `@azure/msal-node` `src/client/ManagedIdentitySources/Imds.ts`.
//! MSAL's own IMDS retry policy (`:189`) is unused: the credential disables
//! MSAL's internal retries (`index.js:79`) and retries in its pipeline.

use super::base_managed_identity_source::{
    get_managed_identity_user_assigned_id_query_parameter_key,
    get_validated_env_variable_url_string, BaseManagedIdentitySource,
    ManagedIdentityRequestParameters,
};
use super::imds_host;
use super::managed_identity_application::{ManagedIdentityId, ManagedIdentityIdType};
use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::Environment;

/// `:24-26`.
const IMDS_TOKEN_PATH: &str = "/metadata/identity/oauth2/token";
const DEFAULT_IMDS_HOST: &str = "http://169.254.169.254";
const IMDS_API_VERSION: &str = "2018-02-01";

/// Maps to: `Imds`.
pub(super) struct Imds {
    identity_endpoint: String,
}

impl Imds {
    /// Maps to: `:85-138` `tryCreate`: the pod identity host when
    /// `AZURE_POD_IDENTITY_AUTHORITY_HOST` is non-empty, in its canonical form
    /// (lowercase, with a trailing slash), else the default endpoint.
    pub(super) fn try_create(env: &Environment) -> Self {
        let identity_endpoint = match env.var("AZURE_POD_IDENTITY_AUTHORITY_HOST").truthy() {
            Some(host) => {
                get_validated_env_variable_url_string(&format!("{host}{IMDS_TOKEN_PATH}"))
            }
            // `DEFAULT_IMDS_ENDPOINT`.
            None => format!("{}{IMDS_TOKEN_PATH}", imds_host(DEFAULT_IMDS_HOST)),
        };
        Self { identity_endpoint }
    }
}

impl BaseManagedIdentitySource for Imds {
    /// Maps to: `:159-192` `createRequest`.
    fn create_request(
        &self,
        resource: &str,
        managed_identity_id: &ManagedIdentityId,
    ) -> ManagedIdentityRequestParameters {
        let mut request = ManagedIdentityRequestParameters::new(self.identity_endpoint.clone());
        request.headers.push(("Metadata", "true".to_owned()));
        request
            .query_parameters
            .push(("api-version", IMDS_API_VERSION.to_owned()));
        request
            .query_parameters
            .push(("resource", resource.to_owned()));
        if managed_identity_id.id_type != ManagedIdentityIdType::SystemAssigned {
            request.query_parameters.push((
                get_managed_identity_user_assigned_id_query_parameter_key(),
                managed_identity_id.id.clone(),
            ));
        }
        request
    }
}

//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/AppService.ts`. App Service is never
//! probed: the credential probes IMDS only.

use super::base_managed_identity_source::{
    BaseManagedIdentitySource, ManagedIdentityRequestParameters,
    get_managed_identity_user_assigned_id_query_parameter_key,
};
use super::managed_identity_application::{ManagedIdentityId, ManagedIdentityIdType};
use crate::azure_identity::Environment;
use crate::azure_identity::js::JsTruthy;

/// `:22`.
const APP_SERVICE_MSI_API_VERSION: &str = "2019-08-01";

/// Maps to: `AppService`.
pub(super) struct AppService {
    identity_endpoint: String,
    identity_header: String,
}

/// Maps to: `:79-90` `getEnvironmentVariables`.
pub(super) fn get_environment_variables(env: &Environment) -> [Option<String>; 2] {
    ["IDENTITY_ENDPOINT", "IDENTITY_HEADER"].map(|name| env.var(name).map(str::to_owned))
}

impl AppService {
    /// Maps to: `:107-148` `tryCreate`: both variables non-empty. The
    /// endpoint check (`:126-132`) only rejects an empty URL, which cannot
    /// reach it, and its canonical form is only logged: the request goes to
    /// the variable as it is.
    pub(super) fn try_create(env: &Environment) -> Option<Self> {
        let [identity_endpoint, identity_header] = get_environment_variables(env);
        Some(Self {
            identity_endpoint: identity_endpoint.truthy()?,
            identity_header: identity_header.truthy()?,
        })
    }
}

impl BaseManagedIdentitySource for AppService {
    /// Maps to: `:162-193` `createRequest`.
    fn create_request(
        &self,
        resource: &str,
        managed_identity_id: &ManagedIdentityId,
    ) -> ManagedIdentityRequestParameters {
        let mut request = ManagedIdentityRequestParameters::new(self.identity_endpoint.clone());
        request
            .headers
            .push(("X-IDENTITY-HEADER", self.identity_header.clone()));
        request
            .query_parameters
            .push(("api-version", APP_SERVICE_MSI_API_VERSION.to_owned()));
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

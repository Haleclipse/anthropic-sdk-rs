//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/CloudShell.ts`, detection only: the
//! token request is not ported (see the module root).

use crate::azure_identity::Environment;
use crate::azure_identity::js::JsTruthy;

/// Maps to: `:74-79` `getEnvironmentVariables`.
pub(super) fn get_environment_variables(env: &Environment) -> [Option<String>; 1] {
    [env.var("MSI_ENDPOINT").map(str::to_owned)]
}

/// Maps to: `:100-148` `tryCreate`, as far as whether it creates the source:
/// `MSI_ENDPOINT` non-empty (the endpoint check only rejects an empty URL).
/// The user-assigned refusal is the caller's.
pub(super) fn try_create(env: &Environment) -> bool {
    env.var("MSI_ENDPOINT").truthy().is_some()
}

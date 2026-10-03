//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/MachineLearning.ts`, detection only: the
//! token request is not ported (see the module root).

use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::Environment;

/// Maps to: `:81-89` `getEnvironmentVariables`.
pub(super) fn get_environment_variables(env: &Environment) -> [Option<String>; 2] {
    ["MSI_ENDPOINT", "MSI_SECRET"].map(|name| env.var(name).map(str::to_owned))
}

/// Maps to: `:108-148` `tryCreate`, as far as whether it creates the source:
/// both variables non-empty (the endpoint check only rejects an empty URL).
pub(super) fn try_create(env: &Environment) -> bool {
    get_environment_variables(env)
        .into_iter()
        .all(|value| value.truthy().is_some())
}

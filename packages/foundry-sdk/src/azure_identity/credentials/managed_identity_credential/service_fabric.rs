//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/ServiceFabric.ts`, detection only: the
//! token request is not ported (see the module root).

use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::Environment;

/// Maps to: `:78-94` `getEnvironmentVariables`.
pub(super) fn get_environment_variables(env: &Environment) -> [Option<String>; 3] {
    [
        "IDENTITY_ENDPOINT",
        "IDENTITY_HEADER",
        "IDENTITY_SERVER_THUMBPRINT",
    ]
    .map(|name| env.var(name).map(str::to_owned))
}

/// Maps to: `:115-165` `tryCreate`, as far as whether it creates the source:
/// all three variables non-empty. The endpoint check (`:131-137`) only
/// rejects an empty URL, which cannot reach it, and a user-assigned identity
/// only logs a warning.
pub(super) fn try_create(env: &Environment) -> bool {
    get_environment_variables(env)
        .into_iter()
        .all(|value| value.truthy().is_some())
}

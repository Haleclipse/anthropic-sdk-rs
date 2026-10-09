//! Maps to: `@azure/msal-node`
//! `src/client/ManagedIdentitySources/AzureArc.ts`, detection only: the token
//! request is not ported (see the module root).

use std::path::Path;

use crate::azure_identity::Environment;
use crate::azure_identity::js::JsTruthy;

/// `:45-47`.
const DEFAULT_AZURE_ARC_IDENTITY_ENDPOINT: &str =
    "http://127.0.0.1:40342/metadata/identity/oauth2/token";
const HIMDS_EXECUTABLE_HELPER_STRING: &str = "N/A: himds executable exists";

/// Maps to: `:59-62` `AZURE_ARC_FILE_DETECTION[process.platform]`: the himds
/// executable on Windows and Linux, nothing elsewhere. MSAL builds the
/// Windows path when the module loads, from `ProgramFiles` (`undefined` when
/// unset); here when a source is detected.
fn file_detection_path(env: &Environment) -> Option<String> {
    if cfg!(windows) {
        let program_files = env.var("ProgramFiles").unwrap_or("undefined");
        Some(format!(
            "{program_files}\\AzureConnectedMachineAgent\\himds.exe"
        ))
    } else if cfg!(target_os = "linux") {
        Some("/opt/azcmagent/bin/himds".to_owned())
    } else {
        None
    }
}

/// Maps to: `:116-150` `getEnvironmentVariables`: `IDENTITY_ENDPOINT` and
/// `IMDS_ENDPOINT`, or, when either is empty or unset and the himds
/// executable exists, the default endpoint and a placeholder. MSAL's
/// `accessSync(path, F_OK | R_OK)` is read as "the file opens for reading".
pub(super) fn get_environment_variables(env: &Environment) -> [Option<String>; 2] {
    let identity_endpoint = env.var("IDENTITY_ENDPOINT");
    let imds_endpoint = env.var("IMDS_ENDPOINT");
    if (identity_endpoint.truthy().is_none() || imds_endpoint.truthy().is_none())
        && file_detection_path(env)
            .is_some_and(|path| std::fs::File::open(Path::new(&path)).is_ok())
    {
        return [
            Some(DEFAULT_AZURE_ARC_IDENTITY_ENDPOINT.to_owned()),
            Some(HIMDS_EXECUTABLE_HELPER_STRING.to_owned()),
        ];
    }
    [
        identity_endpoint.map(str::to_owned),
        imds_endpoint.map(str::to_owned),
    ]
}

/// Maps to: `:171-242` `tryCreate`, as far as whether it creates the source:
/// both values non-empty after file detection (the endpoint checks only
/// reject an empty URL). The user-assigned refusal is the caller's.
pub(super) fn try_create(env: &Environment) -> bool {
    get_environment_variables(env)
        .into_iter()
        .all(|value| value.truthy().is_some())
}

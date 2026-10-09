//! Maps to: `@azure/msal-node` `src/client/ManagedIdentityClient.ts`: which
//! managed identity source this process uses.
//!
//! Two answers are kept, both static as in MSAL and so shared by every
//! credential in the process. The source's name (`sourceName`, `:97-122`)
//! counts a variable as present when it is defined, empty or not; the source
//! itself (`identitySource`, `:128-188`) is chosen by each source's
//! `tryCreate`, which wants the variables non-empty. The two can disagree when
//! a variable is set to the empty string, and CC then probes by the name and
//! sends the request to the source, as this port does.

use std::sync::{Arc, Mutex};

use super::app_service::AppService;
use super::base_managed_identity_source::{
    ManagedIdentityRequest, acquire_token_with_managed_identity,
};
use super::imds::Imds;
use super::managed_identity_application::{ManagedIdentityId, ManagedIdentityIdType};
use super::{
    IdentityClient, MsiError, app_service, azure_arc, cloud_shell, machine_learning, service_fabric,
};
use crate::azure_identity::Environment;
use crate::azure_identity::msal::authentication_result::AuthenticationResult;

/// Maps to: `src/utils/Constants.ts` `ManagedIdentitySourceNames`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManagedIdentitySourceNames {
    AppService,
    AzureArc,
    CloudShell,
    DefaultToImds,
    /// Never detected: the default is `DefaultToImds`.
    #[allow(dead_code)]
    Imds,
    MachineLearning,
    ServiceFabric,
}

impl ManagedIdentitySourceNames {
    fn as_str(self) -> &'static str {
        match self {
            Self::AppService => "AppService",
            Self::AzureArc => "AzureArc",
            Self::CloudShell => "CloudShell",
            Self::DefaultToImds => "DefaultToImds",
            Self::Imds => "Imds",
            Self::MachineLearning => "MachineLearning",
            Self::ServiceFabric => "ServiceFabric",
        }
    }
}

/// The source `selectManagedIdentitySource` creates.
enum IdentitySource {
    AppService(AppService),
    Imds(Imds),
    /// Deviation from MSAL: a source MSAL would create and this port leaves
    /// out.
    Unported(ManagedIdentitySourceNames),
}

/// `ManagedIdentityClient.sourceName`.
static SOURCE_NAME: Mutex<Option<ManagedIdentitySourceNames>> = Mutex::new(None);
/// `ManagedIdentityClient.identitySource`.
static IDENTITY_SOURCE: Mutex<Option<Arc<IdentitySource>>> = Mutex::new(None);

/// `ManagedIdentityClient.sourceName`, once set.
pub(super) fn source_name() -> Option<ManagedIdentitySourceNames> {
    *SOURCE_NAME.lock().unwrap()
}

/// Maps to: `:57-81` `sendManagedIdentityTokenRequest`: the source is chosen
/// on the first request and kept; a choice that throws is not kept.
pub(super) async fn send_managed_identity_token_request(
    env: &Environment,
    managed_identity_request: &ManagedIdentityRequest,
    managed_identity_id: &ManagedIdentityId,
    refresh_access_token: bool,
    network_client: &IdentityClient<'_>,
) -> Result<AuthenticationResult, MsiError> {
    let identity_source = {
        let mut cached = IDENTITY_SOURCE.lock().unwrap();
        match cached.as_ref() {
            Some(source) => Arc::clone(source),
            None => {
                let source = Arc::new(select_managed_identity_source(env, managed_identity_id)?);
                *cached = Some(Arc::clone(&source));
                source
            }
        }
    };
    match identity_source.as_ref() {
        IdentitySource::AppService(source) => {
            acquire_token_with_managed_identity(
                source,
                managed_identity_request,
                managed_identity_id,
                refresh_access_token,
                network_client,
            )
            .await
        }
        IdentitySource::Imds(source) => {
            acquire_token_with_managed_identity(
                source,
                managed_identity_request,
                managed_identity_id,
                refresh_access_token,
                network_client,
            )
            .await
        }
        IdentitySource::Unported(name) => Err(unsupported(*name)),
    }
}

/// Maps to: `:83-91` `allEnvironmentVariablesAreDefined`: `!== undefined`,
/// so an empty variable counts.
fn all_environment_variables_are_defined(environment_variables: &[Option<String>]) -> bool {
    environment_variables.iter().all(Option::is_some)
}

/// Maps to: `:97-122` `getManagedIdentitySource`, which sets `sourceName`.
pub(super) fn get_managed_identity_source(env: &Environment) -> ManagedIdentitySourceNames {
    let name =
        if all_environment_variables_are_defined(&service_fabric::get_environment_variables(env)) {
            ManagedIdentitySourceNames::ServiceFabric
        } else if all_environment_variables_are_defined(&app_service::get_environment_variables(
            env,
        )) {
            ManagedIdentitySourceNames::AppService
        } else if all_environment_variables_are_defined(
            &machine_learning::get_environment_variables(env),
        ) {
            ManagedIdentitySourceNames::MachineLearning
        } else if all_environment_variables_are_defined(&cloud_shell::get_environment_variables(
            env,
        )) {
            ManagedIdentitySourceNames::CloudShell
        } else if all_environment_variables_are_defined(&azure_arc::get_environment_variables(env))
        {
            ManagedIdentitySourceNames::AzureArc
        } else {
            ManagedIdentitySourceNames::DefaultToImds
        };
    *SOURCE_NAME.lock().unwrap() = Some(name);
    name
}

/// Maps to: `:128-188` `selectManagedIdentitySource`, in MSAL's order. Every
/// chain ends in IMDS, so `unableToCreateSource` (`:182-186`) cannot be
/// thrown. Cloud Shell and Azure Arc throw for a user-assigned identity
/// (`CloudShell.ts:132-138`, `AzureArc.ts:226-232`); here that is the same
/// unsupported error, likewise not kept.
fn select_managed_identity_source(
    env: &Environment,
    managed_identity_id: &ManagedIdentityId,
) -> Result<IdentitySource, MsiError> {
    let user_assigned = managed_identity_id.id_type != ManagedIdentityIdType::SystemAssigned;
    if service_fabric::try_create(env) {
        return Ok(IdentitySource::Unported(
            ManagedIdentitySourceNames::ServiceFabric,
        ));
    }
    if let Some(source) = AppService::try_create(env) {
        return Ok(IdentitySource::AppService(source));
    }
    if machine_learning::try_create(env) {
        return Ok(IdentitySource::Unported(
            ManagedIdentitySourceNames::MachineLearning,
        ));
    }
    for (detected, name) in [
        (
            cloud_shell::try_create(env),
            ManagedIdentitySourceNames::CloudShell,
        ),
        (
            azure_arc::try_create(env),
            ManagedIdentitySourceNames::AzureArc,
        ),
    ] {
        if detected {
            return if user_assigned {
                Err(unsupported(name))
            } else {
                Ok(IdentitySource::Unported(name))
            };
        }
    }
    Ok(IdentitySource::Imds(Imds::try_create(env)))
}

/// Deviation from MSAL: the error for a source this port leaves out.
fn unsupported(name: ManagedIdentitySourceNames) -> MsiError {
    MsiError::Unsupported(format!(
        "ManagedIdentityCredential: {} managed identity is not supported by anthropic-sdk-foundry.",
        name.as_str()
    ))
}

#[cfg(test)]
pub(super) fn reset() {
    *SOURCE_NAME.lock().unwrap() = None;
    *IDENTITY_SOURCE.lock().unwrap() = None;
}

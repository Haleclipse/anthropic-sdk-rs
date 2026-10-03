//! Maps to: `@azure/identity` `credentials/defaultAzureCredential.js` and the
//! factories it calls (`credentials/defaultAzureCredentialFunctions.js`), for
//! the three credentials ported (see the module root).

use futures::future::BoxFuture;

use super::azure_cli_credential::AzureCliCredential;
use super::chained_token_credential::ChainedTokenCredential;
use super::environment_credential::EnvironmentCredential;
use super::managed_identity_credential::{ManagedIdentityCredential, ManagedIdentityOptions};
use crate::azure_identity::identity_client::RetryOptions;
use crate::azure_identity::js::JsTruthy;
use crate::azure_identity::{AccessToken, CredentialError, Environment, TokenCredential};

/// Maps to: `DefaultAzureCredentialOptions`, with the two inputs the npm
/// package takes from the Node process instead: its environment and its HTTP
/// stack. None of the npm options are ported (no caller passes one).
#[derive(Clone, Debug, Default)]
pub struct DefaultAzureCredentialOptions {
    /// The `process.env` every credential reads, and the Azure CLI runs with.
    pub env: Environment,
    /// The client the token requests go through; a default `reqwest::Client`
    /// when unset. A client is tied to the async runtime it first runs on, so
    /// pass one built for the runtime the credential will be used on.
    pub http_client: Option<reqwest::Client>,
}

/// Maps to: `DefaultAzureCredential`.
pub struct DefaultAzureCredential(ChainedTokenCredential);

/// One entry of the credential list, built when the chain is.
enum Factory {
    Environment,
    ManagedIdentity {
        send_probe_request: bool,
    },
    AzureCli,
    /// A credential `AZURE_TOKEN_CREDENTIALS` can name that is not ported.
    Unported(&'static str),
}

impl DefaultAzureCredential {
    /// Maps to: `defaultAzureCredential.js:63-143` `new DefaultAzureCredential()`.
    /// The one error is an invalid `AZURE_TOKEN_CREDENTIALS`; a credential that
    /// fails to construct is skipped instead (`:133-141`).
    pub fn new(options: DefaultAzureCredentialOptions) -> Result<Self, CredentialError> {
        let DefaultAzureCredentialOptions { env, http_client } = options;
        let factories = factories(
            env.var("AZURE_TOKEN_CREDENTIALS")
                .map(str::to_owned)
                .truthy(),
        )?;
        let sources = factories
            .into_iter()
            .map(|factory| -> Box<dyn TokenCredential> {
                let built: Result<Box<dyn TokenCredential>, String> = match factory {
                    Factory::Environment => EnvironmentCredential::new(&env, http_client.clone())
                        .map(|credential| Box::new(credential) as _),
                    Factory::ManagedIdentity { send_probe_request } => {
                        create_default_managed_identity_credential(
                            &env,
                            http_client.clone(),
                            send_probe_request,
                        )
                        .map(|credential| Box::new(credential) as _)
                    }
                    Factory::AzureCli => Ok(Box::new(AzureCliCredential::new(env.clone()))),
                    Factory::Unported(name) => Ok(Box::new(UnportedCredential(name))),
                };
                built.unwrap_or_else(|_| Box::new(UnavailableDefaultCredential))
            })
            .collect();
        Ok(Self(ChainedTokenCredential::new(sources)))
    }
}

impl TokenCredential for DefaultAzureCredential {
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        self.0.get_token(scopes)
    }
}

/// `:65-123`: the credential list for `AZURE_TOKEN_CREDENTIALS`, trimmed and
/// lowercased. Unset, empty or all whitespace (falsy once trimmed) is the
/// whole chain of the ported credentials: production first, then development.
fn factories(selection: Option<String>) -> Result<Vec<Factory>, CredentialError> {
    let probed = Factory::ManagedIdentity {
        send_probe_request: true,
    };
    let Some((raw, selected)) = selection
        .map(|raw| {
            let selected = raw.trim().to_lowercase();
            (raw, selected)
        })
        .filter(|(_, selected)| !selected.is_empty())
    else {
        return Ok(vec![Factory::Environment, probed, Factory::AzureCli]);
    };
    Ok(match selected.as_str() {
        // Development: VisualStudioCode, AzureCli, AzurePowerShell,
        // AzureDeveloperCli, Broker.
        "dev" => vec![Factory::AzureCli],
        // Production: Environment, WorkloadIdentity, ManagedIdentity.
        "prod" => vec![Factory::Environment, probed],
        "environmentcredential" => vec![Factory::Environment],
        "workloadidentitycredential" => vec![Factory::Unported("WorkloadIdentityCredential")],
        // `sendProbeRequest: false`, as for a standalone managed identity.
        "managedidentitycredential" => vec![Factory::ManagedIdentity {
            send_probe_request: false,
        }],
        "visualstudiocodecredential" => vec![Factory::Unported("VisualStudioCodeCredential")],
        "azureclicredential" => vec![Factory::AzureCli],
        "azurepowershellcredential" => vec![Factory::Unported("AzurePowerShellCredential")],
        "azuredeveloperclicredential" => vec![Factory::Unported("AzureDeveloperCliCredential")],
        _ => {
            return Err(CredentialError::Other(format!(
                "Invalid value for AZURE_TOKEN_CREDENTIALS = {raw}. Valid values are 'prod' or 'dev' or any of these credentials - EnvironmentCredential, WorkloadIdentityCredential, ManagedIdentityCredential, VisualStudioCodeCredential, AzureCliCredential, AzurePowerShellCredential, AzureDeveloperCliCredential."
            )));
        }
    })
}

/// Maps to: `defaultAzureCredentialFunctions.js:22-58`
/// `createDefaultManagedIdentityCredential`, without the resource-id option CC
/// never passes. `AZURE_CLIENT_ID` is read with `??` but only ever tested for
/// truthiness, so an empty value means no client id.
fn create_default_managed_identity_credential(
    env: &Environment,
    http_client: Option<reqwest::Client>,
    send_probe_request: bool,
) -> Result<ManagedIdentityCredential, String> {
    let client_id = env.var("AZURE_CLIENT_ID").map(str::to_owned);
    let workload_file = env.var("AZURE_FEDERATED_TOKEN_FILE").truthy();
    let tenant_id = env.var("AZURE_TENANT_ID").map(str::to_owned);
    let client_id = client_id.filter(|id| !id.is_empty());
    let options = ManagedIdentityOptions {
        // The workload branch hands the tenant on as well.
        tenant_id: if workload_file.is_some() && client_id.is_some() {
            tenant_id
        } else {
            None
        },
        client_id,
        send_probe_request,
        env: env.clone(),
        http_client,
        // `options.retryOptions ??= { maxRetries: 5, retryDelayInMs: 800 }`.
        retry_options: RetryOptions {
            max_retries: 5,
            retry_delay_ms: 800,
            max_retry_delay_ms: RetryOptions::IDENTITY_CLIENT.max_retry_delay_ms,
        },
    };
    ManagedIdentityCredential::new(options)
}

/// Maps to: `defaultAzureCredential.js:11-22` `UnavailableDefaultCredential`,
/// the stand-in for a credential that failed to construct: it resolves to
/// `null`, so it adds no line to the chain's error.
struct UnavailableDefaultCredential;

impl TokenCredential for UnavailableDefaultCredential {
    fn get_token<'a>(
        &'a self,
        _scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(async { Ok(None) })
    }
}

/// Deviation from the npm package: a credential `AZURE_TOKEN_CREDENTIALS`
/// names that this port leaves out reports itself unavailable, so the chain's
/// error says why instead of the generic "Failed to retrieve a valid token".
struct UnportedCredential(&'static str);

impl TokenCredential for UnportedCredential {
    fn get_token<'a>(
        &'a self,
        _scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        let message = format!("{} is not supported by anthropic-sdk-foundry.", self.0);
        Box::pin(async move { Err(CredentialError::Unavailable(message)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(selection: Option<&str>) -> Vec<&'static str> {
        factories(selection.map(str::to_owned))
            .unwrap()
            .into_iter()
            .map(|factory| match factory {
                Factory::Environment => "env",
                Factory::ManagedIdentity {
                    send_probe_request: true,
                } => "mi+probe",
                Factory::ManagedIdentity {
                    send_probe_request: false,
                } => "mi",
                Factory::AzureCli => "cli",
                Factory::Unported(name) => name,
            })
            .collect()
    }

    /// `defaultAzureCredential.js:63-123`.
    #[test]
    fn credential_list_matches_official_token_credentials_selection() {
        assert_eq!(names(None), ["env", "mi+probe", "cli"]);
        assert_eq!(names(Some("   ")), ["env", "mi+probe", "cli"]);
        assert_eq!(names(Some(" PROD ")), ["env", "mi+probe"]);
        assert_eq!(names(Some("dev")), ["cli"]);
        assert_eq!(names(Some("ManagedIdentityCredential")), ["mi"]);
        assert_eq!(
            names(Some("azurepowershellcredential")),
            ["AzurePowerShellCredential"]
        );
        let Err(error) = factories(Some("Bogus".to_owned())) else {
            panic!("an invalid selection is an error");
        };
        assert_eq!(
            error.to_string(),
            "Invalid value for AZURE_TOKEN_CREDENTIALS = Bogus. Valid values are 'prod' or 'dev' or any of these credentials - EnvironmentCredential, WorkloadIdentityCredential, ManagedIdentityCredential, VisualStudioCodeCredential, AzureCliCredential, AzurePowerShellCredential, AzureDeveloperCliCredential."
        );
    }
}

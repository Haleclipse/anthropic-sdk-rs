//! Maps to: `@azure/identity` `credentials/managedIdentityCredential/index.js`
//! and `utils.js`. The credential hands the token request to `@azure/msal-node`
//! 5.1.1's `ManagedIdentityApplication`, ported in the submodules named after
//! its files.
//!
//! Two sources are ported, App Service and IMDS (the default). The other
//! sources MSAL selects (Service Fabric, Machine Learning, Cloud Shell, Azure
//! Arc), and the token exchange flow the credential runs itself for AKS
//! workload identity, are detected as the npm package detects them and then
//! reported unavailable (a deviation): falling through to IMDS instead would
//! hand an AKS pod its node's identity.
//!
//! As in MSAL, the source and the tokens are kept for the life of the process
//! (`ManagedIdentityClient.sourceName`/`identitySource` and
//! `ManagedIdentityApplication.nodeStorage` are static), whatever credential
//! instance asks.

mod app_service;
mod azure_arc;
mod base_managed_identity_source;
mod cloud_shell;
mod imds;
mod imds_msi;
mod imds_retry_policy;
mod machine_learning;
mod managed_identity_application;
mod managed_identity_client;
mod service_fabric;
mod token_exchange_msi;

use futures::future::BoxFuture;
use serde_json::Value;

use self::imds_retry_policy::MsiRetryConfig;
use self::managed_identity_application::ManagedIdentityApplication;
use self::managed_identity_client::ManagedIdentitySourceNames;
use crate::azure_identity::identity_client::{self, Request, RetryOptions, SendError};
use crate::azure_identity::msal::auth_error::{MsalError, MsalErrorKind};
use crate::azure_identity::msal::network_response::NetworkResponse;
use crate::azure_identity::{
    AccessToken, CredentialError, Environment, TokenCredential, timestamp,
};

/// What `createDefaultManagedIdentityCredential` constructs with.
pub struct ManagedIdentityOptions {
    /// The `process.env` the sources are detected from.
    pub env: Environment,
    /// The client the probe and the token requests go through.
    pub http_client: Option<reqwest::Client>,
    /// User-assigned identity: `AZURE_CLIENT_ID` when non-empty.
    pub client_id: Option<String>,
    /// Only on the workload-identity branch (`AZURE_FEDERATED_TOKEN_FILE` and
    /// a client id). In CC it reaches only the identity client's options,
    /// which the token exchange flow hands to `WorkloadIdentityCredential`;
    /// that flow is not ported, so nothing reads it.
    pub tenant_id: Option<String>,
    pub send_probe_request: bool,
    pub retry_options: RetryOptions,
}

/// Maps to: `ManagedIdentityCredential`.
pub struct ManagedIdentityCredential {
    /// `this.clientId`. An empty id is no id: every read of it is a
    /// truthiness test.
    client_id: Option<String>,
    send_probe_request: bool,
    /// The pipeline retry options of `this.identityClient` (`index.js:68-71`).
    retry_options: RetryOptions,
    /// `this.msiRetryConfig` (`:29-33`), which `imdsRetryPolicy` reads.
    msi_retry_config: MsiRetryConfig,
    managed_identity_app: ManagedIdentityApplication,
    env: Environment,
    http_client: Option<reqwest::Client>,
}

/// `utils.js:7` `serviceFabricErrorMessage`.
const SERVICE_FABRIC_ERROR_MESSAGE: &str = "Specifying a `clientId` or `resourceId` is not supported by the Service Fabric managed identity environment. The managed identity configuration is determined by the Service Fabric cluster resource configuration. See https://aka.ms/servicefabricmi for more information";

impl ManagedIdentityCredential {
    /// Maps to: `index.js:40-123` `new ManagedIdentityCredential(options)`;
    /// `Err` where it throws. Only a client id is ever passed, so the
    /// "only one of clientId, resourceId or objectId" check (`:55-62`) cannot
    /// fire.
    pub fn new(options: ManagedIdentityOptions) -> Result<Self, String> {
        let client_id = options.client_id.filter(|id| !id.is_empty());
        // `:65-67`: `retryOptions.maxRetries` replaces the policy's 5; its
        // 800 ms start delay is not taken from the options.
        let msi_retry_config = MsiRetryConfig {
            max_retries: options.retry_options.max_retries,
            start_delay_ms: 800,
        };
        // `:68-71` and `:88-93`: both identity clients check the authority
        // host in their constructor (`identityClient.js:45-48`), before MSAL
        // detects anything. Neither is given an `authorityHost` option.
        identity_client::base_uri(&options.env, None)?;
        let managed_identity_app =
            ManagedIdentityApplication::new(options.env.clone(), client_id.as_deref());
        // `:94-116`: the two sources that ignore a user-assigned identity
        // refuse one as early as possible. The source is detected here, once
        // per process.
        if client_id.is_some() {
            match managed_identity_app.get_managed_identity_source() {
                ManagedIdentitySourceNames::CloudShell => {
                    return Err("ManagedIdentityCredential: Specifying a user-assigned managed identity is not supported for CloudShell at runtime. When using Managed Identity in CloudShell, omit the clientId, resourceId, and objectId parameters.".to_owned());
                }
                ManagedIdentitySourceNames::ServiceFabric => {
                    return Err(format!(
                        "ManagedIdentityCredential: {SERVICE_FABRIC_ERROR_MESSAGE}"
                    ));
                }
                _ => {}
            }
        }
        Ok(Self {
            client_id,
            send_probe_request: options.send_probe_request,
            retry_options: options.retry_options,
            msi_retry_config,
            managed_identity_app,
            env: options.env,
            http_client: options.http_client,
        })
    }

    /// The `try` block of `getToken` (`index.js:140-197`). The one client
    /// serves the probe and the token request alike.
    async fn get_token_with_msal(
        &self,
        scopes: &[String],
        resource: &str,
    ) -> Result<AccessToken, MsiError> {
        let env = &self.env;
        // `:141`, `:150-164`.
        if token_exchange_msi::is_available(self.client_id.as_deref(), env) {
            return Err(MsiError::Unsupported(
                "ManagedIdentityCredential: The token exchange managed identity (AKS workload identity) is not supported by anthropic-sdk-foundry.".to_owned(),
            ));
        }
        let client = identity_client::http_client(self.http_client.as_ref());
        // `:147-148`: neither name checks that IMDS answers, only that MSAL
        // would try it.
        let identity_source = self.managed_identity_app.get_managed_identity_source();
        let is_imds_msi = matches!(
            identity_source,
            ManagedIdentitySourceNames::DefaultToImds | ManagedIdentitySourceNames::Imds
        );
        // `:165-179`: a fresh probe on every call, before the token cache is
        // looked at.
        if is_imds_msi
            && self.send_probe_request
            && !imds_msi::is_available(scopes, client.as_ref().ok(), env).await?
        {
            return Err(MsiError::Other(
                "Attempted to use the IMDS endpoint, but it is not available.".to_owned(),
            ));
        }
        // `:186-196`.
        let network_client = IdentityClient {
            client: &client,
            retry_options: self.retry_options,
            msi_retry_config: self.msi_retry_config,
        };
        let token = self
            .managed_identity_app
            .acquire_token(resource, &network_client)
            .await?;
        let (access_token, expires_on) =
            ensure_valid_msal_token(token.access_token, token.expires_on)?;
        // `getTime()` of the dates.
        Ok(AccessToken {
            token: access_token,
            expires_on_timestamp: timestamp(expires_on),
            refresh_after_timestamp: token.refresh_on.map(timestamp),
        })
    }
}

impl TokenCredential for ManagedIdentityCredential {
    /// Maps to: `index.js:133-211` `getToken`. Everything that fails inside
    /// the `try` is rethrown as unavailable (`:198-209`), so a managed identity
    /// never halts the chain; only `AuthenticationRequiredError` keeps its
    /// name.
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>> {
        Box::pin(async move {
            // `:135-138`, outside the `try`.
            let Some(resource) =
                map_scopes_to_resource(scopes).filter(|resource| !resource.is_empty())
            else {
                let scopes = serde_json::to_string(scopes).unwrap_or_default();
                return Err(CredentialError::Unavailable(format!(
                    "ManagedIdentityCredential: Multiple scopes are not supported. Scopes: {scopes}"
                )));
            };
            self.get_token_with_msal(scopes, &resource)
                .await
                .map(Some)
                .map_err(MsiError::into_credential_error)
        })
    }
}

/// Maps to: `utils.js:17-32` `mapScopesToResource`: the one scope without
/// its last `/.default` suffix; `None` for any other number of scopes.
fn map_scopes_to_resource(scopes: &[String]) -> Option<String> {
    let [scope] = scopes else {
        return None;
    };
    Some(
        scope
            .strip_suffix(DEFAULT_SCOPE_SUFFIX)
            .unwrap_or(scope)
            .to_owned(),
    )
}

/// `utils.js:3`.
const DEFAULT_SCOPE_SUFFIX: &str = "/.default";

/// Maps to: `index.js:215-233` `ensureValidMsalToken`. `AuthenticationResult`
/// is always there, so only the two field checks remain.
fn ensure_valid_msal_token(
    access_token: String,
    expires_on: Option<f64>,
) -> Result<(String, f64), MsiError> {
    let Some(expires_on) = expires_on else {
        return Err(MsiError::AuthenticationRequired(
            "Response had no \"expiresOn\" property.".to_owned(),
        ));
    };
    if access_token.is_empty() {
        return Err(MsiError::AuthenticationRequired(
            "Response had no \"accessToken\" property.".to_owned(),
        ));
    }
    Ok((access_token, expires_on))
}

/// What fails inside `getToken`'s `try`, as `index.js:198-251` tells the
/// errors apart.
#[derive(Debug)]
enum MsiError {
    /// An MSAL `AuthError` (`ClientAuthError`, `ServerError`,
    /// `InteractionRequiredAuthError`). `error_code` is its `errorCode` when
    /// that is a string.
    Msal {
        error_code: Option<String>,
        message: String,
    },
    /// `AuthenticationRequiredError`, rethrown as it is (`:202-204`).
    AuthenticationRequired(String),
    /// Any other error, by its message.
    Other(String),
    /// Deviation from the npm package: a source this port leaves out.
    /// Reported as it is, not as an authentication failure.
    Unsupported(String),
}

impl MsiError {
    /// `index.js:198-209`.
    fn into_credential_error(self) -> CredentialError {
        if self.is_network_error() {
            return CredentialError::Unavailable(format!(
                "ManagedIdentityCredential: Network unreachable. Message: {}",
                self.message()
            ));
        }
        match self {
            Self::AuthenticationRequired(message) => {
                CredentialError::AuthenticationRequired(message)
            }
            Self::Unsupported(message) => CredentialError::Unavailable(message),
            error => CredentialError::Unavailable(format!(
                "ManagedIdentityCredential: Authentication failed. Message {}",
                error.message()
            )),
        }
    }

    /// Maps to: `index.js:235-252` `isNetworkError`. The probe's system error
    /// codes and the Docker Desktop 403 come from the probe and the token
    /// exchange flow, and neither throws one here (the probe turns every
    /// failure into `false`; the flow is not ported), so MSAL's
    /// `network_error` is the one case left.
    fn is_network_error(&self) -> bool {
        matches!(self, Self::Msal { error_code: Some(code), .. } if code == "network_error")
    }

    fn message(&self) -> &str {
        match self {
            Self::Msal { message, .. }
            | Self::AuthenticationRequired(message)
            | Self::Other(message)
            | Self::Unsupported(message) => message,
        }
    }
}

/// What MSAL throws, as `getToken`'s `catch` sees it: an `AuthError` by its
/// code and message, anything else by its message.
impl From<MsalError> for MsiError {
    fn from(error: MsalError) -> Self {
        match error.kind {
            MsalErrorKind::Other => Self::Other(error.message),
            _ => Self::Msal {
                error_code: error.error_code,
                message: error.message,
            },
        }
    }
}

/// Maps to: the `IdentityClient` MSAL is given as its `networkClient`
/// (`index.js:68-71`): the passed-in transport, the pipeline retry policy, and
/// `imdsRetryPolicy` added per call.
struct IdentityClient<'a> {
    client: &'a Result<reqwest::Client, SendError>,
    retry_options: RetryOptions,
    msi_retry_config: MsiRetryConfig,
}

impl IdentityClient<'_> {
    /// Maps to: `identityClient.js:185-201` `sendGetRequestAsync`: an empty
    /// body is `undefined`, any other is parsed as JSON. `Err` is whatever the
    /// call throws, which MSAL replaces with its own `network_error`.
    async fn send_get_request_async(
        &self,
        url: String,
        headers: Vec<(&'static str, String)>,
    ) -> Result<NetworkResponse<Option<Value>>, String> {
        let client = self
            .client
            .as_ref()
            .map_err(|error| error.message.clone())?;
        let request = Request {
            method: reqwest::Method::GET,
            url,
            headers,
            body: None,
            timeout: None,
        };
        let response =
            imds_retry_policy::send(client, &request, self.retry_options, self.msi_retry_config)
                .await
                .map_err(|error| error.message)?;
        let body = if response.body.is_empty() {
            None
        } else {
            Some(serde_json::from_str(&response.body).map_err(|error| error.to_string())?)
        };
        Ok(NetworkResponse {
            status: response.status,
            body,
        })
    }
}

/// The IMDS host both the probe (`imdsMsi.js:10`) and MSAL's IMDS source
/// (`Imds.ts:25`) default to. Tests point it at a local server, which
/// `AZURE_POD_IDENTITY_AUTHORITY_HOST` cannot do for the probe: that variable
/// skips it.
fn imds_host(default: &'static str) -> String {
    #[cfg(test)]
    if let Some(host) = TEST_IMDS_HOST.lock().unwrap().clone() {
        return host;
    }
    default.to_owned()
}

#[cfg(test)]
static TEST_IMDS_HOST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Forgets the process-wide source, tokens and test IMDS host.
#[cfg(test)]
fn reset_process_state() {
    managed_identity_client::reset();
    managed_identity_application::reset();
    *TEST_IMDS_HOST.lock().unwrap() = None;
}

#[cfg(test)]
mod tests;

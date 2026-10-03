//! Maps to: npm `@azure/identity` 4.13.1, the part a Foundry client needs:
//! `new DefaultAzureCredential()` and `getBearerTokenProvider(credential,
//! 'https://cognitiveservices.azure.com/.default')`, which Claude Code hands to
//! `AnthropicFoundry` as `azureADTokenProvider` (`services/api/client.ts:200-210`).
//! The TS `@anthropic-ai/foundry-sdk` does not depend on `@azure/identity`; this
//! crate carries it behind the `azure-identity` feature, off by default.
//!
//! A minimal port with no Azure crate. Of the eight credentials
//! `DefaultAzureCredential` chains, three are ported, in their order:
//! - `EnvironmentCredential`, client-secret branch only;
//! - `ManagedIdentityCredential`, the IMDS and App Service sources only;
//! - `AzureCliCredential`.
//!
//! The others are left out (WorkloadIdentity, VisualStudioCode, PowerShell,
//! Azure Developer CLI, Broker). VisualStudioCode and Broker need plugins and
//! WorkloadIdentity needs AKS; what the omission changes is that a failed
//! Azure CLI is not followed by PowerShell and `azd`, and the chain's aggregate
//! error lists fewer lines.
//!
//! Two inputs the npm package takes from the Node process are passed in
//! ([`DefaultAzureCredentialOptions`]): the environment ([`Environment`]), and
//! optionally the HTTP client the token requests go through. Without one, a
//! default `reqwest::Client` is used: it takes its proxy from the OS
//! environment by reqwest's rules, not by `@azure/core-rest-pipeline`'s
//! (`HTTPS_PROXY`, `ALL_PROXY`, then `HTTP_PROXY` for every scheme, NO_PROXY
//! without `*`), and not from [`Environment`].

pub(crate) mod constants;
pub mod credentials;
pub mod errors;
pub(crate) mod identity_client;
pub(crate) mod js;
pub(crate) mod msal;
pub mod token_provider;

pub use credentials::default_azure_credential::{
    DefaultAzureCredential, DefaultAzureCredentialOptions,
};
pub use errors::CredentialError;
pub use token_provider::{get_bearer_token_provider, BearerTokenProvider};

use std::ffi::{OsStr, OsString};
use std::sync::Arc;

use futures::future::BoxFuture;

/// Maps to: `@azure/core-auth` `AccessToken`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessToken {
    pub token: String,
    /// Milliseconds since the Unix epoch.
    pub expires_on_timestamp: u64,
    /// Milliseconds since the Unix epoch.
    pub refresh_after_timestamp: Option<u64>,
}

/// Maps to: `@azure/core-auth` `TokenCredential.getToken(scopes, options)`.
/// `Ok(None)` is the `null` a credential may resolve to (the stand-in for one
/// that failed to construct does). The bearer policy, the only caller here,
/// passes `{ enableCae: true }` and no tenant; the credentials that care apply
/// that themselves.
pub trait TokenCredential: Send + Sync {
    fn get_token<'a>(
        &'a self,
        scopes: &'a [String],
    ) -> BoxFuture<'a, Result<Option<AccessToken>, CredentialError>>;
}

/// The `process.env` the credentials read, passed in by the caller. The npm
/// package reads the Node process's environment object directly; this port
/// reads nothing from the process itself, so an application whose environment
/// differs from the OS one (it applies settings after startup) hands over its
/// own. The Azure CLI credential runs `az` with exactly these variables, as
/// `exec` inherits `process.env`.
///
/// When a key repeats, the first entry wins, as `getenv` does. Keys compare
/// exactly, except on Windows, where they compare ignoring ASCII case.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    variables: Arc<[(OsString, OsString)]>,
}

impl Environment {
    pub fn new<I, K, V>(variables: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
    {
        let mut unique: Vec<(OsString, OsString)> = Vec::new();
        for (key, value) in variables {
            let key = key.into();
            if !unique
                .iter()
                .any(|(existing, _)| keys_equal(existing, &key))
            {
                unique.push((key, value.into()));
            }
        }
        Self {
            variables: unique.into(),
        }
    }

    pub fn var_os(&self, key: impl AsRef<OsStr>) -> Option<&OsStr> {
        let key = key.as_ref();
        self.variables
            .iter()
            .find(|(candidate, _)| keys_equal(candidate, key))
            .map(|(_, value)| value.as_os_str())
    }

    /// The value as UTF-8; a value that is not is treated as unset.
    pub fn var(&self, key: impl AsRef<OsStr>) -> Option<&str> {
        self.var_os(key).and_then(OsStr::to_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.variables
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str()))
    }
}

fn keys_equal(left: &OsStr, right: &OsStr) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// `Date.now()`.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// A JS millisecond time (a date's `getTime()`, or a number a credential
/// computes) as [`AccessToken`] holds it. The cast saturates: `NaN` (an
/// invalid date) and a time before the epoch become 0 (expired either way),
/// an infinite or out-of-range one the far future.
pub(crate) fn timestamp(milliseconds: f64) -> u64 {
    milliseconds as u64
}

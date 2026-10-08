//! Port of `@aws-sdk/credential-providers` 3.1020.0's `fromNodeProviderChain`,
//! that is `@aws-sdk/credential-provider-node` 3.972.28's `defaultProvider`:
//! the chain the TS SDK resolves credentials with when it is given no keys
//! (`bedrock-sdk core/auth.ts:19-31`).
//!
//! The npm packages read the Node process: `process.env`, `os.homedir()`,
//! `child_process.exec` and `http`. This port reads what the caller passes:
//! an [`Environment`] and, optionally, the `reqwest::Client` to send with.
//!
//! One file per npm package:
//!
//! | file | npm package |
//! |---|---|
//! | `mod.rs` | `@aws-sdk/credential-provider-node` (`defaultProvider`) |
//! | `property_provider.rs` | `@smithy/property-provider` |
//! | `shared_ini_file_loader.rs` | `@smithy/shared-ini-file-loader` |
//! | `env.rs` | `@aws-sdk/credential-provider-env` |
//! | `ini.rs` | `@aws-sdk/credential-provider-ini` |
//! | `process.rs` | `@aws-sdk/credential-provider-process` |
//! | `http.rs` | `@aws-sdk/credential-provider-http` |
//! | `imds.rs` | `@smithy/credential-provider-imds` |
//!
//! Profiles that assume a role, use a web identity token, SSO or a login
//! session, and the chain's own web identity step, are not resolved yet; they
//! report it instead of falling through to another source.

mod env;
mod http;
mod imds;
mod ini;
mod js;
mod process;
mod property_provider;
mod shared_ini_file_loader;

use std::ffi::{OsStr, OsString};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use anthropic_sdk::core::error::ApiError;
use futures::future::BoxFuture;

use crate::client::AwsCredentialProvider;
use crate::core::auth::AwsCredentials;

pub use property_provider::CredentialsProviderError;

/// The `process.env` the chain reads, passed in by the caller. The npm
/// packages read the Node process's environment object directly; this port
/// reads nothing from the process itself, so an application whose environment
/// differs from the OS one (it applies settings after startup) hands over its
/// own. `credential_process` runs with exactly these variables, as `exec`
/// inherits `process.env`.
///
/// When a key repeats, the first entry wins, as `getenv` does. Keys compare
/// exactly, except on Windows, where they compare ignoring ASCII case.
///
/// Its `Debug` shows only how many variables it holds: an environment carries
/// secrets of every kind.
#[derive(Clone, Default)]
pub struct Environment {
    variables: Arc<[(OsString, OsString)]>,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("variables", &self.variables.len())
            .finish()
    }
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

    /// The process's own environment, as `process.env` is for the TS SDK. The
    /// client reads it when it is given neither keys nor a provider.
    pub fn from_process() -> Self {
        Self::new(anthropic_sdk::internal::env::process_env())
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

    /// `process.env[key]` where JS tests its truthiness: unset and `""` are
    /// both absent.
    pub(crate) fn truthy(&self, key: impl AsRef<OsStr>) -> Option<&str> {
        self.var(key).filter(|value| !value.is_empty())
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

/// What [`from_node_provider_chain`] reads.
#[derive(Clone, Default)]
pub struct NodeProviderChainOptions {
    /// The environment the chain and `credential_process` see.
    pub env: Environment,
    /// The client the network sources send with (container credentials,
    /// instance metadata). npm sends them with Node's `http` module, so
    /// directly, with a one-second connection timeout; `None` builds such a
    /// client. A client of the caller's should not proxy these link-local
    /// endpoints either.
    pub http_client: Option<reqwest::Client>,
    /// TS `init.profile`: the profile to read instead of `AWS_PROFILE`.
    pub profile: Option<String>,
}

impl std::fmt::Debug for NodeProviderChainOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeProviderChainOptions")
            .field("env", &self.env)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "<reqwest::Client>"),
            )
            .field("profile", &self.profile)
            .finish()
    }
}

/// npm's `connectionTimeout` and the IMDS request timeout, `1000`.
const METADATA_TIMEOUT: Duration = Duration::from_millis(1000);

/// The client the network sources use when the caller passes none: no proxy,
/// and npm's one-second connection timeout.
fn metadata_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(METADATA_TIMEOUT)
        .build()
        .unwrap_or_default()
}

/// The credentials a provider resolves, AWS SDK `AwsCredentialIdentity`.
/// `accountId` and `credentialScope` are not kept: Bedrock signing uses
/// neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub expiration: Option<SystemTime>,
}

/// The chain's inputs, shared by every provider (the npm `init` object).
#[derive(Clone)]
pub(crate) struct Init {
    pub env: Environment,
    pub http_client: reqwest::Client,
    pub profile: Option<String>,
}

impl std::fmt::Debug for Init {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Init")
            .field("env", &self.env)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

/// `defaultProvider(init)`: the default chain, memoized as `memoizeChain`
/// does. Credentials without an expiry are kept; credentials that expire are
/// dropped once expired, and are fetched again in the background in their last
/// five minutes while the current ones keep being returned.
///
/// A lookup runs as a task of its own, as npm's promise runs to its end
/// whoever awaits it: a caller that stops waiting (a cancelled request) leaves
/// it to finish for the next one, and a lookup that cannot finish (its runtime
/// shut down, a panic) is an error for its waiters, after which the next call
/// looks up again.
pub fn from_node_provider_chain(options: NodeProviderChainOptions) -> NodeProviderChain {
    let http_client = options.http_client.unwrap_or_else(metadata_http_client);
    NodeProviderChain {
        inner: Arc::new(ChainInner {
            init: Init {
                env: options.env,
                http_client,
                profile: options.profile,
            },
            state: Mutex::new(ChainState::default()),
        }),
    }
}

/// The provider [`from_node_provider_chain`] returns.
#[derive(Clone, Debug)]
pub struct NodeProviderChain {
    inner: Arc<ChainInner>,
}

#[derive(Debug)]
struct ChainInner {
    init: Init,
    state: Mutex<ChainState>,
}

type Lookup =
    futures::future::Shared<BoxFuture<'static, Result<Credentials, CredentialsProviderError>>>;

#[derive(Default)]
struct ChainState {
    credentials: Option<Credentials>,
    /// `activeLock`: the lookup that callers finding no credentials share,
    /// its result (an error too) going to every one of them.
    active: Option<Lookup>,
    /// `passiveLock`: a background refresh is running.
    refreshing: bool,
}

impl std::fmt::Debug for ChainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainState")
            .field("credentials", &self.credentials.as_ref().map(|_| "***"))
            .field("active", &self.active.is_some())
            .field("refreshing", &self.refreshing)
            .finish()
    }
}

/// `credentialsTreatedAsExpired`: inside the last five minutes.
fn treated_as_expired(credentials: &Credentials) -> bool {
    credentials.expiration.is_some_and(|expiration| {
        expiration
            .duration_since(SystemTime::now())
            .map_or(true, |left| left < Duration::from_millis(300_000))
    })
}

fn expired(credentials: &Credentials) -> bool {
    credentials
        .expiration
        .is_some_and(|expiration| expiration < SystemTime::now())
}

impl NodeProviderChain {
    /// `memoizeChain`'s `provider`.
    async fn provide(&self) -> Result<Credentials, CredentialsProviderError> {
        use futures::FutureExt as _;
        let lookup = {
            let mut state = self.inner.state.lock().unwrap();
            if state.credentials.as_ref().is_some_and(expired) {
                state.credentials = None;
            }
            match (&state.active, &state.credentials) {
                (Some(active), _) => active.clone(),
                (None, Some(credentials)) if !treated_as_expired(credentials) => {
                    return Ok(credentials.clone());
                }
                // Still valid but expiring: refresh in the background and
                // answer with the current ones.
                (None, Some(credentials)) => {
                    let current = credentials.clone();
                    if !std::mem::replace(&mut state.refreshing, true) {
                        spawn_refresh(&self.inner);
                    }
                    return Ok(current);
                }
                (None, None) => {
                    // The task holds the inputs only, and the lookup a weak
                    // reference, so neither keeps the chain alive.
                    let init = self.inner.init.clone();
                    let task = tokio::spawn(async move { default_chain(&init).await });
                    let chain = Arc::downgrade(&self.inner);
                    let lookup = async move {
                        let result = task.await.unwrap_or_else(|error| {
                            Err(CredentialsProviderError::stop(format!(
                                "The AWS credential lookup did not finish: {error}"
                            )))
                        });
                        if let Some(inner) = chain.upgrade() {
                            let mut state = inner.state.lock().unwrap();
                            state.active = None;
                            if let Ok(credentials) = &result {
                                state.credentials = Some(credentials.clone());
                            }
                        }
                        result
                    }
                    .boxed()
                    .shared();
                    state.active = Some(lookup.clone());
                    lookup
                }
            }
        };
        lookup.await
    }
}

/// `passiveLock`: the background lookup, whose result answers from then on.
/// The flag is cleared however the task ends, cancelled or panicking too.
fn spawn_refresh(inner: &Arc<ChainInner>) {
    struct ClearRefreshing(Weak<ChainInner>);
    impl Drop for ClearRefreshing {
        fn drop(&mut self) {
            if let Some(inner) = self.0.upgrade() {
                if let Ok(mut state) = inner.state.lock() {
                    state.refreshing = false;
                }
            }
        }
    }
    let init = inner.init.clone();
    let chain = Arc::downgrade(inner);
    tokio::spawn(async move {
        let _clear = ClearRefreshing(chain.clone());
        let refreshed = default_chain(&init).await;
        let Some(inner) = chain.upgrade() else {
            return;
        };
        match refreshed {
            Ok(credentials) => inner.state.lock().unwrap().credentials = Some(credentials),
            Err(error) => tracing::warn!(%error, "AWS credential refresh failed"),
        }
    });
}

impl AwsCredentialProvider for NodeProviderChain {
    fn get_credentials(&self) -> BoxFuture<'_, Result<AwsCredentials, ApiError>> {
        Box::pin(async move {
            let credentials = self
                .provide()
                .await
                .map_err(|error| ApiError::Sdk(error.message().to_owned()))?;
            Ok(AwsCredentials {
                access_key_id: credentials.access_key_id,
                secret_access_key: credentials.secret_access_key,
                // The TS signer sends the token only when it is truthy.
                session_token: credentials.session_token.filter(|token| !token.is_empty()),
            })
        })
    }
}

/// The links of `defaultProvider`, in order; `internalCreateChain` moves on
/// past an error only when it says `tryNextLink`.
async fn default_chain(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    let mut last = None;
    for link in 0..7 {
        let result = match link {
            0 => from_env_link(init),
            1 => Err(CredentialsProviderError::new(
                "Skipping SSO provider in default chain (inputs do not include SSO fields).",
            )),
            2 => ini::from_ini(init).await,
            3 => process::from_process(init).await,
            4 => from_token_file_link(init),
            5 => remote_provider(init).await,
            _ => Err(CredentialsProviderError::stop(
                "Could not load credentials from any providers",
            )),
        };
        match result {
            Ok(credentials) => return Ok(credentials),
            Err(error) if error.try_next_link() => last = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last.expect("the last link always stops the chain"))
}

/// The first link: `fromEnv`, unless a profile is selected.
fn from_env_link(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    // `init.profile ?? process.env.AWS_PROFILE`, then a truthiness test.
    let profile = match &init.profile {
        Some(profile) => Some(profile.as_str()),
        None => init.env.var(shared_ini_file_loader::ENV_PROFILE),
    };
    if profile.is_some_and(|profile| !profile.is_empty()) {
        if init.env.truthy(env::ENV_KEY).is_some() && init.env.truthy(env::ENV_SECRET).is_some() {
            warn_multiple_credential_sources();
        }
        return Err(CredentialsProviderError::new(
            "AWS_PROFILE is set, skipping fromEnv provider.",
        ));
    }
    env::from_env(&init.env)
}

/// npm warns once per process.
fn warn_multiple_credential_sources() {
    static EMITTED: std::sync::Once = std::sync::Once::new();
    EMITTED.call_once(|| {
        tracing::warn!(
            "@aws-sdk/credential-provider-node - defaultProvider::fromEnv WARNING:\n    \
             Multiple credential sources detected: \n    \
             Both AWS_PROFILE and the pair AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY static credentials are set.\n    \
             This SDK will proceed with the AWS_PROFILE value.\n    \n    \
             However, a future version may change this behavior to prefer the ENV static credentials.\n    \
             Please ensure that your environment only sets either the AWS_PROFILE or the\n    \
             AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY pair.\n"
        );
    });
}

/// `fromTokenFile` reads `AWS_WEB_IDENTITY_TOKEN_FILE` and `AWS_ROLE_ARN`;
/// assuming that role is not resolved yet.
fn from_token_file_link(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    if init.env.truthy("AWS_WEB_IDENTITY_TOKEN_FILE").is_some()
        && init.env.truthy("AWS_ROLE_ARN").is_some()
    {
        return Err(CredentialsProviderError::stop(
            "Web identity credentials (AWS_WEB_IDENTITY_TOKEN_FILE) are not resolved yet.",
        ));
    }
    Err(CredentialsProviderError::new(
        "Web identity configuration not specified",
    ))
}

/// `remoteProvider`: container credentials when their variables are set,
/// else instance metadata unless `AWS_EC2_METADATA_DISABLED` turns it off.
async fn remote_provider(init: &Init) -> Result<Credentials, CredentialsProviderError> {
    if init.env.truthy(imds::ENV_CMDS_RELATIVE_URI).is_some()
        || init.env.truthy(imds::ENV_CMDS_FULL_URI).is_some()
    {
        // `chain(fromHttp(init), fromContainerMetadata(init))`. `fromHttp`
        // checks its URL as the chain is built, so an error there is the
        // link's own: the container metadata provider never runs.
        let endpoint = http::container_endpoint(init)?;
        return match http::from_http(init, &endpoint).await {
            Ok(credentials) => Ok(credentials),
            Err(error) if error.try_next_link() => imds::from_container_metadata(init).await,
            Err(error) => Err(error),
        };
    }
    if init
        .env
        .truthy(imds::ENV_IMDS_DISABLED)
        .is_some_and(|value| value != "false")
    {
        return Err(CredentialsProviderError::new(
            "EC2 Instance Metadata Service access disabled",
        ));
    }
    imds::from_instance_metadata(init).await
}

#[cfg(test)]
mod tests;

// Google Application Default Credentials for the Vertex client.
//
// Maps to: Node `google-auth-library` `new GoogleAuth({ scopes:
// 'https://www.googleapis.com/auth/cloud-platform' })`, the TS Vertex default
// (`client.ts:106-111`), with `getRequestHeaders()` and `projectId`. The
// credential flows follow the Rust `google-cloud-auth` crate, cut down to what
// Vertex needs.
//
// Sources, in Node's order (`googleauth.ts` `getApplicationDefaultAsync`):
//   1. `GOOGLE_APPLICATION_CREDENTIALS`
//   2. the gcloud well-known file
//      (`$HOME/.config/gcloud/application_default_credentials.json`, or
//      `%APPDATA%\gcloud\...` on Windows)
//   3. the GCE metadata server
//
// Credential file types:
// - `authorized_user`
// - `service_account`
// - `impersonated_service_account`
// - `external_account`, with a `file` or `url` credential source
//
// Not supported yet, with an error pointing at `TokenProvider`:
// - executable- and AWS-sourced external accounts
// - `external_account_authorized_user`
//
// The npm package reads the Node process's `process.env`; this port reads the
// `Environment` the caller passes (`GoogleAuthOptions::env`), and sends with
// the caller's client when it passes one. `GoogleAuth::default()` passes the
// process's own environment and a default client.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anthropic_sdk::core::error::ApiError;
use serde::Deserialize;
use serde_json::Value;

use crate::client::TokenProvider;

const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_STS_TOKEN_URL: &str = "https://sts.googleapis.com/v1/token";
const DEFAULT_LIFETIME_SECS: u64 = 3600;
/// Node `eagerRefreshThresholdMillis` default: refresh five minutes early.
const REFRESH_THRESHOLD: Duration = Duration::from_secs(300);
/// gcp-metadata `requestTimeout()` outside a known GCP environment.
const METADATA_PING_TIMEOUT: Duration = Duration::from_secs(3);

/// Node `GoogleAuthExceptionMessages.NO_ADC_FOUND`.
pub const NO_ADC_FOUND: &str = "Could not load the default credentials. Browse to https://cloud.google.com/docs/authentication/getting-started for more information.";

/// The `process.env` [`GoogleAuth`] reads, passed in by the caller. The npm
/// packages (google-auth-library, gcp-metadata) read the Node process's
/// environment object directly; this port reads nothing from the process
/// itself, so an application whose environment differs from the OS one (it
/// applies settings after startup) hands over its own.
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

    /// The process's own environment, as `process.env` is for the npm
    /// packages.
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
    fn truthy(&self, key: impl AsRef<OsStr>) -> Option<&str> {
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

/// What [`GoogleAuth::new`] reads, besides what Node's `GoogleAuthOptions`
/// carries (the scope is always `cloud-platform`, as the TS Vertex SDK and CC
/// ask for).
#[derive(Clone, Default)]
pub struct GoogleAuthOptions {
    /// The environment the credential search, the metadata server settings
    /// and the quota project come from.
    pub env: Environment,
    /// The client the token, STS, IAM and metadata requests send with;
    /// `None` uses a default `reqwest::Client`, whose proxy comes from the OS
    /// environment by reqwest's rules, not gaxios'.
    pub http_client: Option<reqwest::Client>,
}

impl std::fmt::Debug for GoogleAuthOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleAuthOptions")
            .field("env", &self.env)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "<reqwest::Client>"),
            )
            .finish()
    }
}

/// Application Default Credentials: the [`TokenProvider`] a Vertex client uses
/// when given neither `access_token` nor `token_provider`.
///
/// The credential file is read when this is built; the metadata server and
/// token endpoints are contacted on the first request. Tokens are cached and
/// fetched again five minutes before they expire. `request_headers` adds
/// `x-goog-user-project` for the quota project (`GOOGLE_CLOUD_QUOTA_PROJECT`,
/// else the file's `quota_project_id`), as Node's `getRequestHeaders()` does.
pub struct GoogleAuth {
    env: Environment,
    source: Result<Source, String>,
    project_id: Option<String>,
    quota_project_id: Option<String>,
    metadata_base: String,
    /// Node `GoogleAuth.checkIsGCE`: decided once, then kept.
    on_gce: tokio::sync::OnceCell<bool>,
    http: OnceLock<reqwest::Client>,
    cached: tokio::sync::Mutex<Option<(String, SystemTime)>>,
}

impl std::fmt::Debug for GoogleAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleAuth")
            .field("env", &self.env)
            .field("source", &self.source.as_ref().map(Source::kind))
            .field("project_id", &self.project_id)
            .field("quota_project_id", &self.quota_project_id)
            .finish_non_exhaustive()
    }
}

enum Source {
    AuthorizedUser {
        client_id: String,
        client_secret: String,
        refresh_token: String,
        token_uri: String,
    },
    ServiceAccount {
        client_email: String,
        private_key: String,
        private_key_id: Option<String>,
        token_uri: String,
    },
    Impersonated {
        url: String,
        delegates: Vec<String>,
        lifetime_secs: u64,
        source: Box<Source>,
    },
    ExternalAccount(ExternalAccount),
    Metadata,
}

impl Source {
    fn kind(&self) -> &'static str {
        match self {
            Self::AuthorizedUser { .. } => "authorized_user",
            Self::ServiceAccount { .. } => "service_account",
            Self::Impersonated { .. } => "impersonated_service_account",
            Self::ExternalAccount(_) => "external_account",
            Self::Metadata => "metadata",
        }
    }
}

struct ExternalAccount {
    audience: String,
    subject_token_type: String,
    token_url: String,
    subject: SubjectSource,
    impersonation_url: Option<String>,
    lifetime_secs: u64,
    client_auth: Option<(String, String)>,
    workforce_pool_user_project: Option<String>,
}

enum SubjectSource {
    File {
        path: String,
        format: SubjectFormat,
    },
    Url {
        url: String,
        headers: HashMap<String, String>,
        format: SubjectFormat,
    },
}

enum SubjectFormat {
    Text,
    Json(String),
}

impl GoogleAuth {
    /// Find the default credentials, reading `options.env` and the credential
    /// file now. A missing or unreadable file is reported on the first
    /// request, as Node reports it from `getClient()`.
    pub fn new(options: GoogleAuthOptions) -> Self {
        let GoogleAuthOptions { env, http_client } = options;
        let loaded = load_credentials_file(&env);
        let (source, file_project_id, file_quota_project_id) = match loaded {
            Ok(Some((source, project_id, quota))) => (Ok(source), project_id, quota),
            Ok(None) => (Ok(Source::Metadata), None, None),
            Err(error) => (Err(error), None, None),
        };
        // `#prepareAndCacheClient`'s `process.env['GOOGLE_CLOUD_QUOTA_PROJECT']
        // || null`, over the file's own.
        let quota_project_id = env
            .truthy("GOOGLE_CLOUD_QUOTA_PROJECT")
            .map(str::to_owned)
            .or(file_quota_project_id);
        // TS Vertex: `authClient.projectId ?? authHeaders['x-goog-user-project']`.
        let project_id = file_project_id.or_else(|| quota_project_id.clone());
        let http = OnceLock::new();
        if let Some(client) = http_client {
            let _ = http.set(client);
        }
        Self {
            metadata_base: metadata_base_url(&env),
            env,
            source,
            project_id,
            quota_project_id,
            on_gce: tokio::sync::OnceCell::new(),
            http,
            cached: tokio::sync::Mutex::new(None),
        }
    }

    fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(reqwest::Client::new)
    }

    async fn token(&self) -> Result<String, ApiError> {
        let mut cached = self.cached.lock().await;
        if let Some((token, expiry)) = cached.as_ref() {
            if *expiry > SystemTime::now() + REFRESH_THRESHOLD {
                return Ok(token.clone());
            }
        }
        let source = self
            .source
            .as_ref()
            .map_err(|error| ApiError::Sdk(error.clone()))?;
        let (token, expiry) = self.fetch(source).await.map_err(ApiError::Sdk)?;
        *cached = Some((token.clone(), expiry));
        Ok(token)
    }

    fn fetch<'a>(
        &'a self,
        source: &'a Source,
    ) -> futures::future::BoxFuture<'a, Result<(String, SystemTime), String>> {
        Box::pin(async move {
            match source {
                Source::AuthorizedUser {
                    client_id,
                    client_secret,
                    refresh_token,
                    token_uri,
                } => {
                    let body = form(&[
                        ("client_id", client_id),
                        ("client_secret", client_secret),
                        ("refresh_token", refresh_token),
                        ("grant_type", "refresh_token"),
                    ]);
                    self.post_form(token_uri, body, None).await
                }
                Source::ServiceAccount {
                    client_email,
                    private_key,
                    private_key_id,
                    token_uri,
                } => {
                    let assertion = service_account_jwt(
                        client_email,
                        private_key,
                        private_key_id.as_deref(),
                        token_uri,
                    )?;
                    let body = form(&[
                        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                        ("assertion", &assertion),
                    ]);
                    self.post_form(token_uri, body, None).await
                }
                Source::Impersonated {
                    url,
                    delegates,
                    lifetime_secs,
                    source,
                } => {
                    let (source_token, _) = self.fetch(source).await?;
                    self.impersonate(url, delegates, *lifetime_secs, &source_token)
                        .await
                }
                Source::ExternalAccount(account) => self.external_account_token(account).await,
                Source::Metadata => self.metadata_token().await,
            }
        })
    }

    async fn post_form(
        &self,
        url: &str,
        body: String,
        basic_auth: Option<&(String, String)>,
    ) -> Result<(String, SystemTime), String> {
        let mut request = self
            .http()
            .post(url)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body);
        if let Some((user, password)) = basic_auth {
            request = request.basic_auth(user, Some(password));
        }
        let json = send_json(request, url).await?;
        oauth_token(&json, url)
    }

    /// IAM Credentials `generateAccessToken`, as Node's `Impersonated` client.
    async fn impersonate(
        &self,
        url: &str,
        delegates: &[String],
        lifetime_secs: u64,
        source_token: &str,
    ) -> Result<(String, SystemTime), String> {
        let request = self
            .http()
            .post(url)
            .bearer_auth(source_token)
            .json(&serde_json::json!({
                "scope": [SCOPE],
                "delegates": delegates,
                "lifetime": format!("{lifetime_secs}s"),
            }));
        let json = send_json(request, url).await?;
        let token = json
            .get("accessToken")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{url} returned no accessToken"))?;
        let expiry = json
            .get("expireTime")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_utc)
            .unwrap_or_else(|| SystemTime::now() + Duration::from_secs(lifetime_secs));
        Ok((token.to_owned(), expiry))
    }

    /// STS token exchange (RFC 8693), then impersonation if configured, as
    /// Node's `BaseExternalAccountClient`.
    async fn external_account_token(
        &self,
        account: &ExternalAccount,
    ) -> Result<(String, SystemTime), String> {
        let subject_token = self.subject_token(&account.subject).await?;
        let mut params = vec![
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange".to_owned(),
            ),
            ("audience", account.audience.clone()),
            ("scope", SCOPE.to_owned()),
            (
                "requested_token_type",
                "urn:ietf:params:oauth:token-type:access_token".to_owned(),
            ),
            ("subject_token", subject_token),
            ("subject_token_type", account.subject_token_type.clone()),
        ];
        if account.client_auth.is_none() {
            if let Some(project) = &account.workforce_pool_user_project {
                params.push((
                    "options",
                    serde_json::json!({ "userProject": project }).to_string(),
                ));
            }
        }
        let pairs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (sts_token, sts_expiry) = self
            .post_form(
                &account.token_url,
                form(&pairs),
                account.client_auth.as_ref(),
            )
            .await?;
        match &account.impersonation_url {
            Some(url) => {
                self.impersonate(url, &[], account.lifetime_secs, &sts_token)
                    .await
            }
            None => Ok((sts_token, sts_expiry)),
        }
    }

    async fn subject_token(&self, subject: &SubjectSource) -> Result<String, String> {
        let (raw, format, origin) = match subject {
            SubjectSource::File { path, format } => {
                let raw = std::fs::read_to_string(path).map_err(|error| {
                    format!("Unable to read the subject token file {path}: {error}")
                })?;
                (raw, format, path.as_str())
            }
            SubjectSource::Url {
                url,
                headers,
                format,
            } => {
                let mut request = self.http().get(url);
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                let response = request.send().await.map_err(|error| {
                    format!("Failed to fetch the subject token from {url}: {error}")
                })?;
                let status = response.status();
                let raw = response.text().await.map_err(|error| error.to_string())?;
                if !status.is_success() {
                    return Err(format!(
                        "Failed to fetch the subject token from {url}: {status} {raw}"
                    ));
                }
                (raw, format, url.as_str())
            }
        };
        let token = match format {
            SubjectFormat::Text => raw,
            SubjectFormat::Json(field) => serde_json::from_str::<Value>(&raw)
                .ok()
                .and_then(|json| json.get(field).and_then(Value::as_str).map(str::to_owned))
                .ok_or_else(|| {
                    format!("The subject token from {origin} has no string field `{field}`")
                })?,
        };
        if token.is_empty() {
            return Err(format!("The subject token from {origin} is empty"));
        }
        Ok(token)
    }

    async fn metadata_token(&self) -> Result<(String, SystemTime), String> {
        let on_gce = self
            .on_gce
            .get_or_try_init(|| self.metadata_available())
            .await?;
        if !on_gce {
            return Err(NO_ADC_FOUND.to_owned());
        }
        let url = format!(
            "{}/instance/service-accounts/default/token?scopes={}",
            self.metadata_base,
            url::form_urlencoded::byte_serialize(SCOPE.as_bytes()).collect::<String>()
        );
        let request = self.http().get(&url).header("Metadata-Flavor", "Google");
        let json = send_json(request, &url).await?;
        oauth_token(&json, &url)
    }

    /// Node `GoogleAuth._checkIsGCE`: `getGCPResidency() || await
    /// isAvailable()`, so residency decides before the detection mode is
    /// read.
    async fn metadata_available(&self) -> Result<bool, String> {
        if gcp_residency(&self.env) {
            return Ok(true);
        }
        // gcp-metadata `isAvailable()`: `if (process.env.METADATA_SERVER_DETECTION)`,
        // then `.trim()`, so a value of spaces is an unknown mode.
        let detection = self
            .env
            .truthy("METADATA_SERVER_DETECTION")
            .map(|value| value.trim().to_lowercase());
        match detection.as_deref() {
            Some("assume-present") => return Ok(true),
            Some("none") => return Ok(false),
            Some("bios-only") => return Ok(gcp_residency(&self.env)),
            Some("ping-only") | None => {}
            Some(other) => {
                return Err(format!(
                    "Unknown `METADATA_SERVER_DETECTION` env variable. Got `{other}`, but it should be `assume-present`, `none`, `bios-only`, `ping-only`, or unset"
                ));
            }
        }
        let response = self
            .http()
            .get(format!("{}/instance", self.metadata_base))
            .header("Metadata-Flavor", "Google")
            .timeout(METADATA_PING_TIMEOUT)
            .send()
            .await;
        Ok(response.is_ok_and(|response| {
            response.status().is_success()
                && response
                    .headers()
                    .get("metadata-flavor")
                    .is_some_and(|value| value == "Google")
        }))
    }
}

impl Default for GoogleAuth {
    /// The process's own environment and a default client, as the npm
    /// package reads `process.env`.
    fn default() -> Self {
        Self::new(GoogleAuthOptions {
            env: Environment::from_process(),
            http_client: None,
        })
    }
}

impl TokenProvider for GoogleAuth {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>> {
        Box::pin(self.token())
    }

    fn request_headers(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<HashMap<String, String>, ApiError>> {
        Box::pin(async move {
            let mut headers = HashMap::from([(
                "authorization".to_owned(),
                format!("Bearer {}", self.token().await?),
            )]);
            if let Some(project) = &self.quota_project_id {
                headers.insert("x-goog-user-project".to_owned(), project.clone());
            }
            Ok(headers)
        })
    }

    fn project_id(&self) -> Option<String> {
        self.project_id.clone()
    }
}

// ── Credential files ────────────────────────────────────────────────────────

type LoadedFile = (Source, Option<String>, Option<String>);

fn load_credentials_file(env: &Environment) -> Result<Option<LoadedFile>, String> {
    let from_env = env
        .truthy("GOOGLE_APPLICATION_CREDENTIALS")
        .or_else(|| env.truthy("google_application_credentials"));
    if let Some(path) = from_env {
        return read_credentials_file(path).map(Some).map_err(|error| {
            format!(
                "Unable to read the credential file specified by the GOOGLE_APPLICATION_CREDENTIALS environment variable: {error}"
            )
        });
    }
    let Some(path) = well_known_file(env).filter(|path| path.exists()) else {
        return Ok(None);
    };
    read_credentials_file(&path.to_string_lossy()).map(Some)
}

fn well_known_file(env: &Environment) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        PathBuf::from(env.truthy("APPDATA")?)
    } else {
        PathBuf::from(env.truthy("HOME")?).join(".config")
    };
    Some(
        base.join("gcloud")
            .join("application_default_credentials.json"),
    )
}

fn read_credentials_file(path: &str) -> Result<LoadedFile, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
    let project_id = string_field(&json, "project_id");
    let quota_project_id = string_field(&json, "quota_project_id");
    Ok((parse_source(&json)?, project_id, quota_project_id))
}

fn string_field(json: &Value, name: &str) -> Option<String> {
    json.get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required(json: &Value, name: &str) -> Result<String, String> {
    string_field(json, name)
        .ok_or_else(|| format!("The credential file is missing the `{name}` field"))
}

fn parse_source(json: &Value) -> Result<Source, String> {
    match json.get("type").and_then(Value::as_str) {
        Some("authorized_user") => Ok(Source::AuthorizedUser {
            client_id: required(json, "client_id")?,
            client_secret: required(json, "client_secret")?,
            refresh_token: required(json, "refresh_token")?,
            token_uri: string_field(json, "token_uri")
                .unwrap_or_else(|| DEFAULT_TOKEN_URI.to_owned()),
        }),
        Some("service_account") => Ok(Source::ServiceAccount {
            client_email: required(json, "client_email")?,
            private_key: required(json, "private_key")?,
            private_key_id: string_field(json, "private_key_id"),
            token_uri: string_field(json, "token_uri")
                .unwrap_or_else(|| DEFAULT_TOKEN_URI.to_owned()),
        }),
        Some("impersonated_service_account") => {
            let source = json
                .get("source_credentials")
                .ok_or("The credential file is missing the `source_credentials` field")?;
            Ok(Source::Impersonated {
                url: required(json, "service_account_impersonation_url")?,
                delegates: json
                    .get("delegates")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                lifetime_secs: DEFAULT_LIFETIME_SECS,
                source: Box::new(parse_source(source)?),
            })
        }
        Some("external_account") => parse_external_account(json).map(Source::ExternalAccount),
        Some(other) => Err(format!(
            "Unsupported credential type `{other}`. Pass a `TokenProvider` to authenticate with it."
        )),
        None => Err("The credential file has no `type` field".to_owned()),
    }
}

fn parse_external_account(json: &Value) -> Result<ExternalAccount, String> {
    let source = json
        .get("credential_source")
        .ok_or("The credential file is missing the `credential_source` field")?;
    let format = match source.get("format") {
        Some(format) if format.get("type").and_then(Value::as_str) == Some("json") => {
            SubjectFormat::Json(required(format, "subject_token_field_name")?)
        }
        _ => SubjectFormat::Text,
    };
    let subject = if let Some(path) = string_field(source, "file") {
        SubjectSource::File { path, format }
    } else if let Some(url) = string_field(source, "url") {
        let headers = source
            .get("headers")
            .and_then(Value::as_object)
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        SubjectSource::Url {
            url,
            headers,
            format,
        }
    } else if source.get("executable").is_some() {
        return Err(
            "Executable-sourced external account credentials are not supported yet. Pass a `TokenProvider` to authenticate with them."
                .to_owned(),
        );
    } else if string_field(source, "environment_id").is_some_and(|id| id.starts_with("aws")) {
        return Err(
            "AWS-sourced external account credentials are not supported yet. Pass a `TokenProvider` to authenticate with them."
                .to_owned(),
        );
    } else {
        return Err("The external account `credential_source` has no `file` or `url`".to_owned());
    };
    let client_auth = match (
        string_field(json, "client_id"),
        string_field(json, "client_secret"),
    ) {
        (Some(id), secret) => Some((id, secret.unwrap_or_default())),
        _ => None,
    };
    Ok(ExternalAccount {
        audience: required(json, "audience")?,
        subject_token_type: required(json, "subject_token_type")?,
        token_url: string_field(json, "token_url")
            .unwrap_or_else(|| DEFAULT_STS_TOKEN_URL.to_owned()),
        subject,
        impersonation_url: string_field(json, "service_account_impersonation_url"),
        lifetime_secs: json
            .pointer("/service_account_impersonation/token_lifetime_seconds")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_LIFETIME_SECS),
        client_auth,
        workforce_pool_user_project: string_field(json, "workforce_pool_user_project"),
    })
}

// ── Service account JWT ─────────────────────────────────────────────────────

/// The RS256 JWT bearer assertion Node's `gtoken` exchanges at `token_uri`.
fn service_account_jwt(
    client_email: &str,
    private_key: &str,
    private_key_id: Option<&str>,
    token_uri: &str,
) -> Result<String, String> {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    let mut header = serde_json::json!({ "alg": "RS256", "typ": "JWT" });
    if let Some(kid) = private_key_id {
        header["kid"] = Value::from(kid);
    }
    let claims = serde_json::json!({
        "iss": client_email,
        "scope": SCOPE,
        "aud": token_uri,
        "iat": now,
        "exp": now + DEFAULT_LIFETIME_SECS,
    });
    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let signature = sign_rs256(private_key, signing_input.as_bytes())?;
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature)
    ))
}

/// Signs with the process's rustls `CryptoProvider`, as `google-cloud-auth`
/// does, so no crypto backend is added: the application's (e.g. ring) is used.
#[cfg(not(target_os = "android"))]
fn sign_rs256(private_key_pem: &str, message: &[u8]) -> Result<Vec<u8>, String> {
    use rustls::pki_types::PrivateKeyDer;
    use rustls::pki_types::pem::PemObject;

    let provider = rustls::crypto::CryptoProvider::get_default().ok_or(
        "Signing with a service account key needs a rustls CryptoProvider; install one (for example ring) with `CryptoProvider::install_default()`",
    )?;
    let key = PrivateKeyDer::from_pem_slice(private_key_pem.as_bytes())
        .map_err(|error| format!("Invalid service account private key: {error}"))?;
    let signing_key = provider
        .key_provider
        .load_private_key(key)
        .map_err(|error| format!("Invalid service account private key: {error}"))?;
    let signer = signing_key
        .choose_scheme(&[rustls::SignatureScheme::RSA_PKCS1_SHA256])
        .ok_or("The service account private key cannot sign RS256")?;
    signer
        .sign(message)
        .map_err(|error| format!("Failed to sign the service account assertion: {error}"))
}

#[cfg(target_os = "android")]
fn sign_rs256(_private_key_pem: &str, _message: &[u8]) -> Result<Vec<u8>, String> {
    Err("Service account keys are not supported on Android, which has no rustls CryptoProvider. Pass a `TokenProvider` to authenticate with one.".to_owned())
}

// ── Metadata server ─────────────────────────────────────────────────────────

/// gcp-metadata `getBaseUrl()`: `GCE_METADATA_IP`, else `GCE_METADATA_HOST`,
/// else `169.254.169.254`, over plain HTTP unless a scheme is given.
fn metadata_base_url(env: &Environment) -> String {
    let host = env
        .truthy("GCE_METADATA_IP")
        .or_else(|| env.truthy("GCE_METADATA_HOST"))
        .unwrap_or("169.254.169.254");
    let host = if host.starts_with("http://") || host.starts_with("https://") {
        host.to_owned()
    } else {
        format!("http://{host}")
    };
    format!("{}/computeMetadata/v1", host.trim_end_matches('/'))
}

/// gcp-metadata `getGCPResidency()`: serverless environment variables or a
/// Google BIOS on Linux. (The MAC-address check is not ported.)
fn gcp_residency(env: &Environment) -> bool {
    let serverless = ["CLOUD_RUN_JOB", "FUNCTION_NAME", "K_SERVICE"]
        .iter()
        .any(|name| env.truthy(name).is_some());
    serverless
        || (cfg!(target_os = "linux")
            && std::fs::metadata("/sys/class/dmi/id/bios_date").is_ok()
            && std::fs::read_to_string("/sys/class/dmi/id/bios_vendor")
                .is_ok_and(|vendor| vendor.contains("Google")))
}

// ── HTTP helpers ────────────────────────────────────────────────────────────

fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

async fn send_json(request: reqwest::RequestBuilder, url: &str) -> Result<Value, String> {
    let response = request
        .send()
        .await
        .map_err(|error| format!("Request to {url} failed: {error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("Request to {url} failed: {error}"))?;
    if !status.is_success() {
        return Err(format!("{url} returned {status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|error| format!("{url} returned invalid JSON: {error}"))
}

#[derive(Deserialize)]
struct OAuthToken {
    access_token: String,
    expires_in: Option<u64>,
}

fn oauth_token(json: &Value, url: &str) -> Result<(String, SystemTime), String> {
    let token: OAuthToken = serde_json::from_value(json.clone())
        .map_err(|error| format!("{url} returned no access token: {error}"))?;
    let lifetime = Duration::from_secs(token.expires_in.unwrap_or(DEFAULT_LIFETIME_SECS));
    Ok((token.access_token, SystemTime::now() + lifetime))
}

/// Parses the `YYYY-MM-DDTHH:MM:SS[.fff]Z` timestamps IAM Credentials returns.
fn parse_rfc3339_utc(value: &str) -> Option<SystemTime> {
    let value = value.strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let time = time.split('.').next()?;
    let mut time = time.split(':').map(str::parse::<u64>);
    let (hour, minute, second) = (time.next()?.ok()?, time.next()?.ok()?, time.next()?.ok()?);
    // Days from the civil date (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = u64::try_from(era * 146_097 + day_of_era - 719_468).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(days * 86_400 + hour * 3600 + minute * 60 + second))
}

#[cfg(test)]
mod tests {
    use super::parse_rfc3339_utc;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn parses_iam_expire_time() {
        assert_eq!(
            parse_rfc3339_utc("2026-09-30T12:34:56Z"),
            Some(UNIX_EPOCH + Duration::from_secs(1_790_771_696))
        );
        assert_eq!(
            parse_rfc3339_utc("1970-01-01T00:00:01.500Z"),
            Some(UNIX_EPOCH + Duration::from_secs(1))
        );
        assert_eq!(parse_rfc3339_utc("2026-09-30 12:34:56"), None);
    }
}

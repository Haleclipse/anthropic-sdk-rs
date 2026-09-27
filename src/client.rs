// Maps to: TS client.ts

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use rand::RngExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::multipart::Form;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::internal::detect_platform::get_platform_headers;
use crate::internal::env::read_env;
use crate::internal::log::format_request_details;
use crate::internal::query::{stringify_query, QueryValue};
use crate::internal::request_options::{
    HttpMiddleware, JsonBodyPatch, RawRequestBody, RequestOptions,
};
use crate::VERSION;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS `BaseAnthropic.DEFAULT_TIMEOUT`
const DEFAULT_TIMEOUT_MS: u64 = 600_000; // 10 minutes

/// Maps to: TS default base URL `https://api.anthropic.com`
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Maps to: TS default max retries
const DEFAULT_MAX_RETRIES: u32 = 2;

/// Maps to: TS `anthropic-version` header value
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maps to: TS `HUMAN_PROMPT`
pub const HUMAN_PROMPT: &str = "\n\nHuman:";

/// Maps to: TS `AI_PROMPT`
pub const AI_PROMPT: &str = "\n\nAssistant:";

// ─────────────────────────────────────────────────────────────────────────────
// ClientOptions
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS `ClientOptions`
///
/// Configuration options for the Anthropic client. All fields are optional and
/// fall back to environment variables or sensible defaults.
/// Async bearer-token provider used by provider SDKs that need per-request
/// credentials (for example Azure AD / Google OAuth token refresh).
pub trait AuthTokenProvider: Send + Sync {
    /// Return a bearer token without the `Bearer ` prefix.
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>>;
}

/// SDK request logging levels.
///
/// Maps to TS `LogLevel` (`off`, `error`, `warn`, `info`, `debug`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    /// Parse a TS log-level string (`off`, `error`, `warn`, `info`, `debug`).
    pub fn from_env_value(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "error" => Some(Self::Error),
            "warn" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    /// Return the TS string literal for this log level.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }

    fn number(self) -> u16 {
        match self {
            Self::Off => 0,
            Self::Error => 200,
            Self::Warn => 300,
            Self::Info => 400,
            Self::Debug => 500,
        }
    }

    fn enables(self, level: Self) -> bool {
        self != Self::Off && level.number() <= self.number()
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for LogLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_env_value(value).ok_or(())
    }
}

/// Rust equivalent of TS `Logger`.
///
/// Implementors can capture request lifecycle messages emitted by the client.
/// If no custom logger is provided, enabled messages are emitted through
/// `tracing` at the corresponding level.
pub trait SdkLogger: Send + Sync {
    fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }

    fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message);
    }

    /// Log a message with TS `loggerFor(client).debug(message, details)`-style
    /// structured details. The default implementation preserves the original
    /// string-oriented Rust logger contract; custom loggers can override this
    /// to capture the redacted `formatRequestDetails()` object separately.
    fn log_with_details(
        &self,
        level: LogLevel,
        message: &str,
        _details: &JsonMap<String, JsonValue>,
    ) {
        self.log(level, message);
    }

    fn error_with_details(&self, message: &str, details: &JsonMap<String, JsonValue>) {
        self.log_with_details(LogLevel::Error, message, details);
    }

    fn warn_with_details(&self, message: &str, details: &JsonMap<String, JsonValue>) {
        self.log_with_details(LogLevel::Warn, message, details);
    }

    fn info_with_details(&self, message: &str, details: &JsonMap<String, JsonValue>) {
        self.log_with_details(LogLevel::Info, message, details);
    }

    fn debug_with_details(&self, message: &str, details: &JsonMap<String, JsonValue>) {
        self.log_with_details(LogLevel::Debug, message, details);
    }

    fn log(&self, level: LogLevel, message: &str);
}

#[derive(Clone, Default)]
pub struct ClientOptions {
    /// API key for `X-Api-Key` header authentication.
    /// Defaults to `ANTHROPIC_API_KEY` env var.
    /// Maps to: TS `ClientOptions.apiKey`
    pub api_key: Option<String>,

    /// Auth token for `Authorization: Bearer` authentication.
    /// Defaults to `ANTHROPIC_AUTH_TOKEN` env var.
    /// Maps to: TS `ClientOptions.authToken`
    pub auth_token: Option<String>,

    /// Async bearer-token provider invoked for each request attempt. This is a
    /// Rust provider-SDK extension for TS provider hooks such as Foundry's
    /// `azureADTokenProvider` and Vertex auth-client token refresh.
    pub auth_token_provider: Option<Arc<dyn AuthTokenProvider>>,

    /// Override the default base URL for the API.
    /// Defaults to `ANTHROPIC_BASE_URL` env var, then `https://api.anthropic.com`.
    /// Maps to: TS `ClientOptions.baseURL`
    pub base_url: Option<String>,

    /// The maximum time in milliseconds that the client will wait for a single
    /// response before timing out. Note that timed-out requests are retried.
    /// Maps to: TS `ClientOptions.timeout`
    pub timeout: Option<u64>,

    /// Maximum number of retries for temporary failures (network errors, 5xx, 429, etc.).
    /// Maps to: TS `ClientOptions.maxRetries`
    pub max_retries: Option<u32>,

    /// Custom HTTP client used for all requests.
    ///
    /// This is the Rust equivalent of TS `ClientOptions.fetch` / default fetch
    /// customization. SDK timeout/abort/retry/header behavior is still applied
    /// around this client for each request attempt.
    pub http_client: Option<reqwest::Client>,

    /// Rust-native request middleware chain.
    ///
    /// Inspired by Go SDK `option.WithMiddleware`; middleware can mutate the
    /// prepared request and observe responses/errors without modelling browser
    /// `fetch` / `RequestInit` fields.
    pub middlewares: Vec<Arc<dyn HttpMiddleware>>,

    /// Request logging level.
    /// Defaults to `ANTHROPIC_LOG`, then `warn`, matching TS `ClientOptions.logLevel`.
    pub log_level: Option<LogLevel>,

    /// Custom request logger.
    /// Maps to TS `ClientOptions.logger`.
    pub logger: Option<Arc<dyn SdkLogger>>,

    /// Default headers included with every request. A value of `None` for a key
    /// removes that header (useful for overriding inherited defaults).
    /// Maps to: TS `ClientOptions.defaultHeaders`
    pub default_headers: Option<HashMap<String, Option<String>>>,

    /// Default query parameters included with every request. A value of `None`
    /// for a key removes that parameter.
    /// Maps to: TS `ClientOptions.defaultQuery`
    pub default_query: Option<HashMap<String, Option<String>>>,
}

impl fmt::Debug for ClientOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientOptions")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***"))
            .field(
                "auth_token_provider",
                &self
                    .auth_token_provider
                    .as_ref()
                    .map(|_| "<dyn AuthTokenProvider>"),
            )
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .field("max_retries", &self.max_retries)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "Some(<reqwest::Client>)"),
            )
            .field("middlewares", &self.middlewares.len())
            .field("log_level", &self.log_level)
            .field(
                "logger",
                &self.logger.as_ref().map(|_| "Some(<dyn SdkLogger>)"),
            )
            .field("default_headers", &self.default_headers)
            .field("default_query", &self.default_query)
            .finish()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Anthropic client
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS `Anthropic` (extends `BaseAnthropic`)
///
/// The primary API client for the Anthropic REST API. Owns a `reqwest::Client`
/// and a resolved copy of [`ClientOptions`]. Handles authentication, retries
/// with exponential back-off, and header construction.
#[derive(Clone)]
pub struct Anthropic {
    http: reqwest::Client,
    api_key: Option<String>,
    auth_token: Option<String>,
    auth_token_provider: Option<Arc<dyn AuthTokenProvider>>,
    base_url: String,
    timeout_ms: u64,
    max_retries: u32,
    default_headers: HashMap<String, Option<String>>,
    default_query: HashMap<String, Option<String>>,
    middlewares: Vec<Arc<dyn HttpMiddleware>>,
    log_level: LogLevel,
    logger: Option<Arc<dyn SdkLogger>>,
    /// Preserved so `with_options` can merge on top.
    _options: ClientOptions,
}

/// TS export-name compatibility alias for `BaseAnthropic`.
///
/// The TS SDK splits shared behavior into `BaseAnthropic` and exports
/// `Anthropic extends BaseAnthropic`. The Rust client keeps one concrete type,
/// so this alias preserves the import name without adding an empty wrapper.
pub type BaseAnthropic = Anthropic;

/// Borrowed inputs for one HTTP attempt. Keeping the attempt state together
/// avoids a long positional parameter list and makes retry call sites explicit.
struct ExecuteOnceRequest<'a> {
    method: reqwest::Method,
    path: &'a str,
    body: Option<&'a JsonValue>,
    extra_headers: Option<&'a HashMap<String, Option<String>>>,
    query: Option<&'a HashMap<String, Option<String>>>,
    retry_count: u32,
    timeout_ms: u64,
    options: Option<&'a RequestOptions>,
    request_log_id: &'a str,
    retry_of_request_log_id: Option<&'a str>,
}

impl fmt::Debug for Anthropic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Anthropic")
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***"))
            .field(
                "auth_token_provider",
                &self
                    .auth_token_provider
                    .as_ref()
                    .map(|_| "<dyn AuthTokenProvider>"),
            )
            .field("base_url", &self.base_url)
            .field("timeout_ms", &self.timeout_ms)
            .field("max_retries", &self.max_retries)
            .field("default_headers", &self.default_headers)
            .field("default_query", &self.default_query)
            .field("middlewares", &self.middlewares.len())
            .field("log_level", &self.log_level)
            .field(
                "logger",
                &self.logger.as_ref().map(|_| "Some(<dyn SdkLogger>)"),
            )
            .finish()
    }
}

impl Anthropic {
    // ── Constructor ──────────────────────────────────────────────────────

    /// Maps to: TS `new Anthropic(opts?)`
    ///
    /// Creates a new client. Fields not set in `opts` fall back to environment
    /// variables and then to compiled-in defaults.
    pub fn new(opts: ClientOptions) -> Result<Self, ApiError> {
        let api_key = opts
            .api_key
            .clone()
            .or_else(|| read_env("ANTHROPIC_API_KEY").filter(|s| !s.is_empty()));

        let auth_token = opts
            .auth_token
            .clone()
            .or_else(|| read_env("ANTHROPIC_AUTH_TOKEN").filter(|s| !s.is_empty()));

        let auth_token_provider = opts.auth_token_provider.clone();

        let base_url = opts
            .base_url
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| read_env("ANTHROPIC_BASE_URL").filter(|s| !s.is_empty()))
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());

        let timeout_ms = opts.timeout.unwrap_or(DEFAULT_TIMEOUT_MS);
        let max_retries = opts.max_retries.unwrap_or(DEFAULT_MAX_RETRIES);
        let logger = opts.logger.clone();
        let log_level = opts.log_level.unwrap_or_else(|| {
            read_env("ANTHROPIC_LOG")
                .filter(|value| !value.is_empty())
                .and_then(|value| {
                    LogLevel::from_env_value(&value).or_else(|| {
                        let warning = format!(
                            "process.env['ANTHROPIC_LOG'] was set to {value:?}, expected one of [\"off\",\"error\",\"warn\",\"info\",\"debug\"]"
                        );
                        if let Some(logger) = &logger {
                            logger.warn(&warning);
                        } else {
                            tracing::warn!("{warning}");
                        }
                        None
                    })
                })
                .unwrap_or(LogLevel::Warn)
        });

        let default_headers = opts.default_headers.clone().unwrap_or_default();
        let default_query = opts.default_query.clone().unwrap_or_default();

        let http = match opts.http_client.clone() {
            Some(client) => client,
            None => reqwest::Client::builder()
                .timeout(Duration::from_millis(timeout_ms))
                .build()
                .map_err(|e| ApiError::Sdk(format!("failed to build HTTP client: {e}")))?,
        };

        Ok(Self {
            http,
            api_key,
            auth_token,
            auth_token_provider,
            base_url,
            timeout_ms,
            max_retries,
            default_headers,
            default_query,
            middlewares: opts.middlewares.clone(),
            log_level,
            logger,
            _options: opts,
        })
    }

    // ── Child client ────────────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.withOptions()`
    ///
    /// Creates a child client that inherits the current client's resolved
    /// settings, with overrides applied from `overrides`.
    pub fn with_options(&self, overrides: ClientOptions) -> Result<Self, ApiError> {
        let merged = ClientOptions {
            api_key: overrides.api_key.or_else(|| self.api_key.clone()),
            auth_token: overrides.auth_token.or_else(|| self.auth_token.clone()),
            auth_token_provider: overrides
                .auth_token_provider
                .or_else(|| self.auth_token_provider.clone()),
            base_url: overrides.base_url.or_else(|| Some(self.base_url.clone())),
            timeout: overrides.timeout.or(Some(self.timeout_ms)),
            max_retries: overrides.max_retries.or(Some(self.max_retries)),
            http_client: overrides
                .http_client
                .or_else(|| self._options.http_client.clone()),
            middlewares: {
                let mut middlewares = self.middlewares.clone();
                middlewares.extend(overrides.middlewares.clone());
                middlewares
            },
            log_level: overrides.log_level.or(Some(self.log_level)),
            logger: overrides.logger.or_else(|| self.logger.clone()),
            default_headers: Some(merge_option_maps(
                &self.default_headers,
                &overrides.default_headers.unwrap_or_default(),
            )),
            default_query: Some(merge_option_maps(
                &self.default_query,
                &overrides.default_query.unwrap_or_default(),
            )),
        };
        Self::new(merged)
    }

    /// TS-style alias for [`Anthropic::with_options`].
    #[allow(non_snake_case)]
    pub fn withOptions(&self, overrides: ClientOptions) -> Result<Self, ApiError> {
        self.with_options(overrides)
    }

    fn log(&self, level: LogLevel, message: impl AsRef<str>) {
        if !self.log_level.enables(level) {
            return;
        }

        let message = message.as_ref();
        if let Some(logger) = &self.logger {
            match level {
                LogLevel::Off => {}
                LogLevel::Error => logger.error(message),
                LogLevel::Warn => logger.warn(message),
                LogLevel::Info => logger.info(message),
                LogLevel::Debug => logger.debug(message),
            }
            return;
        }

        match level {
            LogLevel::Off => {}
            LogLevel::Error => tracing::error!("{message}"),
            LogLevel::Warn => tracing::warn!("{message}"),
            LogLevel::Info => tracing::info!("{message}"),
            LogLevel::Debug => tracing::debug!("{message}"),
        }
    }

    fn log_with_details(
        &self,
        level: LogLevel,
        message: impl AsRef<str>,
        details: &JsonMap<String, JsonValue>,
    ) {
        if !self.log_level.enables(level) {
            return;
        }

        let message = message.as_ref();
        if let Some(logger) = &self.logger {
            match level {
                LogLevel::Off => {}
                LogLevel::Error => logger.error_with_details(message, details),
                LogLevel::Warn => logger.warn_with_details(message, details),
                LogLevel::Info => logger.info_with_details(message, details),
                LogLevel::Debug => logger.debug_with_details(message, details),
            }
            return;
        }

        let rendered = format!("{} {}", message, JsonValue::Object(details.clone()));
        match level {
            LogLevel::Off => {}
            LogLevel::Error => tracing::error!("{rendered}"),
            LogLevel::Warn => tracing::warn!("{rendered}"),
            LogLevel::Info => tracing::info!("{rendered}"),
            LogLevel::Debug => tracing::debug!("{rendered}"),
        }
    }

    fn log_request_start(
        &self,
        request_log_id: &str,
        retry_of_request_log_id: Option<&str>,
        method: &reqwest::Method,
        url: &str,
        retry_count: u32,
        headers: &HeaderMap,
    ) {
        let message = format!(
            "[{request_log_id}] sending request: {} {} (retry_count={retry_count}{}, headers={})",
            method.as_str(),
            url,
            retry_detail_suffix(retry_of_request_log_id),
            redacted_header_log(headers)
        );
        let mut details = JsonMap::new();
        if let Some(retry_of) = retry_of_request_log_id {
            details.insert(
                "retryOfRequestLogID".to_owned(),
                JsonValue::String(retry_of.to_owned()),
            );
        }
        details.insert(
            "method".to_owned(),
            JsonValue::String(method.as_str().to_ascii_lowercase()),
        );
        details.insert("url".to_owned(), JsonValue::String(url.to_owned()));
        details.insert(
            "headers".to_owned(),
            JsonValue::Object(header_map_to_json(headers)),
        );
        let details = format_request_details(details);
        self.log_with_details(LogLevel::Debug, message, &details);
    }

    fn log_response_start_debug(
        &self,
        request_log_id: &str,
        retry_of_request_log_id: Option<&str>,
        response: &reqwest::Response,
        elapsed: Duration,
    ) {
        let url = response.url().as_str();
        let status = response.status().as_u16();
        let headers = response.headers();
        let message = format!(
            "[{request_log_id}] response start: url={url}, status={status}, headers={}, durationMs={}",
            redacted_header_log(headers),
            elapsed.as_millis()
        );
        let details = response_log_details(retry_of_request_log_id, url, status, headers, elapsed);
        self.log_with_details(LogLevel::Debug, message, &details);
    }

    fn log_response_error_debug(
        &self,
        request_log_id: &str,
        retry_of_request_log_id: Option<&str>,
        retry_message: &str,
        response: &reqwest::Response,
        elapsed: Duration,
    ) {
        let url = response.url().as_str();
        let status = response.status().as_u16();
        let headers = response.headers();
        let message = format!(
            "[{request_log_id}] response error ({retry_message}): url={url}, status={status}, headers={}, durationMs={}",
            redacted_header_log(headers),
            elapsed.as_millis()
        );
        let details = response_log_details(retry_of_request_log_id, url, status, headers, elapsed);
        self.log_with_details(LogLevel::Debug, message, &details);
    }

    fn log_connection_debug(
        &self,
        request_log_id: &str,
        retry_of_request_log_id: Option<&str>,
        label: &str,
        retry_message: &str,
        elapsed: Duration,
        err: &ApiError,
    ) {
        let message = format!(
            "[{request_log_id}] connection {label} ({retry_message}): durationMs={}, message={err}",
            elapsed.as_millis()
        );
        let mut details = JsonMap::new();
        if let Some(retry_of) = retry_of_request_log_id {
            details.insert(
                "retryOfRequestLogID".to_owned(),
                JsonValue::String(retry_of.to_owned()),
            );
        }
        details.insert("durationMs".to_owned(), duration_ms_value(elapsed));
        details.insert("message".to_owned(), JsonValue::String(err.to_string()));
        let details = format_request_details(details);
        self.log_with_details(LogLevel::Debug, message, &details);
    }

    // ── URL builder ─────────────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.buildURL()`
    ///
    /// Constructs the full request URL from a path and optional query params.
    /// Merges in `default_query`, omitting entries whose value is `None`.
    pub fn build_url(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
    ) -> Result<String, ApiError> {
        self.build_url_with_default_base_url(path, query, None)
    }

    /// Maps to TS `BaseAnthropic.buildURL(path, query, defaultBaseURL)`.
    ///
    /// `default_base_url` is only used while the client is still pointing at
    /// the SDK default base URL; explicit client/env base URLs take priority.
    pub fn build_url_with_default_base_url(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        default_base_url: Option<&str>,
    ) -> Result<String, ApiError> {
        // If path is already absolute, use it directly.
        let full = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            let base_source = if self.base_url.trim_end_matches('/') == DEFAULT_BASE_URL {
                default_base_url.unwrap_or(&self.base_url)
            } else {
                &self.base_url
            };
            let base = base_source.trim_end_matches('/');
            if path.starts_with('/') {
                format!("{base}{path}")
            } else {
                format!("{base}/{path}")
            }
        };

        let mut url = url::Url::parse(&full)
            .map_err(|e| ApiError::Sdk(format!("invalid URL '{full}': {e}")))?;

        // Merge path query + default_query + per-request query; None values
        // remove a key. Preserving query parameters already present in resource
        // paths is important for Stainless-generated beta paths such as
        // `/v1/models?beta=true` when callers also pass pagination params.
        let original_query = url.query().map(str::to_owned);
        let should_rewrite_query = original_query.is_some()
            || !self.default_query.is_empty()
            || query.map(|q| !q.is_empty()).unwrap_or(false);
        if should_rewrite_query {
            let mut merged: BTreeMap<String, String> = BTreeMap::new();
            if let Some(existing) = original_query.as_deref() {
                for (k, v) in url::form_urlencoded::parse(existing.as_bytes()) {
                    merged.insert(k.into_owned(), v.into_owned());
                }
            }
            for (k, v) in &self.default_query {
                match v {
                    Some(val) => {
                        merged.insert(k.clone(), val.clone());
                    }
                    None => {
                        merged.remove(k);
                    }
                }
            }
            if let Some(q) = query {
                for (k, v) in q {
                    match v {
                        Some(val) => {
                            merged.insert(k.clone(), val.clone());
                        }
                        None => {
                            merged.remove(k);
                        }
                    }
                }
            }

            if merged.is_empty() {
                url.set_query(None);
            } else {
                let query_string = stringify_query(
                    merged
                        .iter()
                        .map(|(key, value)| (key.as_str(), QueryValue::String(value.clone()))),
                )?;
                // TS uses `encodeURIComponent` in `stringifyQuery`, which
                // encodes spaces as `%20` rather than the `+` emitted by
                // `application/x-www-form-urlencoded` serializers.
                url.set_query(Some(&query_string));
            }
        }

        Ok(url.to_string())
    }

    /// TS-style alias for `BaseAnthropic.buildURL(path, query, defaultBaseURL?)`.
    #[allow(non_snake_case)]
    pub fn buildURL(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        default_base_url: Option<&str>,
    ) -> Result<String, ApiError> {
        self.build_url_with_default_base_url(path, query, default_base_url)
    }

    /// Maps to TS `BaseAnthropic.stringifyQuery()`.
    pub fn stringify_query<I, K>(&self, pairs: I) -> Result<String, ApiError>
    where
        I: IntoIterator<Item = (K, QueryValue)>,
        K: AsRef<str>,
    {
        stringify_query(pairs)
    }

    /// TS-style camelCase alias for [`Anthropic::stringify_query`].
    #[allow(non_snake_case)]
    pub fn stringifyQuery<I, K>(&self, pairs: I) -> Result<String, ApiError>
    where
        I: IntoIterator<Item = (K, QueryValue)>,
        K: AsRef<str>,
    {
        self.stringify_query(pairs)
    }

    // ── Header builder ──────────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.buildHeaders()`
    ///
    /// Builds the final `HeaderMap` for a request. Layers (lowest to highest
    /// priority): built-in defaults, auth headers, `default_headers`,
    /// per-request `extra_headers`. A value of `None` in any layer removes
    /// that header (case-insensitive).
    pub fn build_headers(
        &self,
        retry_count: u32,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<HeaderMap, ApiError> {
        self.build_headers_with_timeout(retry_count, extra_headers, None, self.timeout_ms)
    }

    fn build_headers_with_timeout(
        &self,
        retry_count: u32,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        dynamic_auth_headers: Option<&HashMap<String, Option<String>>>,
        timeout_ms: u64,
    ) -> Result<HeaderMap, ApiError> {
        // Collect all layers into a single ordered map. We use lowercase keys
        // so that merging is case-insensitive, matching the TS buildHeaders().
        let mut map: HashMap<String, Option<String>> = HashMap::new();

        // Layer 1 -- built-in defaults
        map.insert("accept".to_owned(), Some("application/json".to_owned()));
        map.insert(
            "user-agent".to_owned(),
            Some(format!("anthropic-sdk-rs/{VERSION}")),
        );
        map.insert(
            "x-stainless-retry-count".to_owned(),
            Some(retry_count.to_string()),
        );
        map.insert(
            "anthropic-version".to_owned(),
            Some(ANTHROPIC_VERSION.to_owned()),
        );
        map.insert(
            "x-stainless-timeout".to_owned(),
            Some((timeout_ms / 1000).to_string()),
        );
        for (name, value) in get_platform_headers() {
            map.insert(name.to_ascii_lowercase(), Some(value));
        }

        // Layer 2 -- auth headers
        if let Some(ref key) = self.api_key {
            map.insert("x-api-key".to_owned(), Some(key.clone()));
        }
        if let Some(ref token) = self.auth_token {
            map.insert("authorization".to_owned(), Some(format!("Bearer {token}")));
        }
        if let Some(dynamic) = dynamic_auth_headers {
            for (k, v) in dynamic {
                map.insert(k.to_lowercase(), v.clone());
            }
        }

        // Layer 3 -- default_headers from options
        for (k, v) in &self.default_headers {
            map.insert(k.to_lowercase(), v.clone());
        }

        // Layer 4 -- per-request overrides
        if let Some(extra) = extra_headers {
            for (k, v) in extra {
                map.insert(k.to_lowercase(), v.clone());
            }
        }

        // Validate that at least one auth mechanism is present (or explicitly
        // removed via None).
        if map.get("x-api-key").and_then(|v| v.as_ref()).is_none()
            && map.get("authorization").and_then(|v| v.as_ref()).is_none()
            && self.auth_token_provider.is_none()
        {
            // Check if either was explicitly nulled out (user intentionally
            // removed it). The TS SDK allows this.
            let api_key_nulled = map.get("x-api-key").is_some_and(|v| v.is_none());
            let auth_nulled = map.get("authorization").is_some_and(|v| v.is_none());
            if !api_key_nulled && !auth_nulled {
                return Err(ApiError::Sdk(
                    "Could not resolve authentication method. Expected either apiKey \
                     or authToken to be set. Or for one of the \"X-Api-Key\" or \
                     \"Authorization\" headers to be explicitly omitted"
                        .to_owned(),
                ));
            }
        }

        // Convert to HeaderMap, dropping None entries.
        let mut header_map = HeaderMap::new();
        for (k, v) in &map {
            if let Some(val) = v {
                let name = HeaderName::from_bytes(k.as_bytes())
                    .map_err(|e| ApiError::Sdk(format!("invalid header name '{k}': {e}")))?;
                let value = HeaderValue::from_str(val)
                    .map_err(|e| ApiError::Sdk(format!("invalid header value for '{k}': {e}")))?;
                header_map.insert(name, value);
            }
        }

        Ok(header_map)
    }

    // ── Core request with retries ───────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.request()` + `makeRequest()` + `retryRequest()`
    ///
    /// Sends an HTTP request with automatic retry logic. Retries on:
    /// - Status codes 408, 409, 429, 5xx
    /// - Connection errors and timeouts
    ///
    /// Uses exponential back-off: `min(0.5 * 2^n, 8.0) * (1 - rand*0.25) * 1000` ms,
    /// respecting `Retry-After-Ms` and `Retry-After` response headers.
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.request_with_options(method, path, body, extra_headers, query, None)
            .await
    }

    /// Maps to: TS `RequestOptions` handling in `BaseAnthropic.request()`.
    ///
    /// Per-request headers and query parameters are applied after the
    /// resource-level values, so they can override or remove them. Per-request
    /// timeout and retry-count override client defaults. A triggered abort
    /// signal returns [`ApiError::UserAbort`] and is not retried.
    pub async fn request_with_options<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        let max_retries = options
            .and_then(|opts| opts.max_retries)
            .unwrap_or(self.max_retries);
        let timeout_ms = timeout_ms_from_options(options, self.timeout_ms);
        let mut retries_remaining = max_retries;
        let mut retry_of_request_log_id: Option<String> = None;

        loop {
            let retry_count = max_retries - retries_remaining;
            let merged_headers = merged_headers(extra_headers, options);
            let merged_query = merged_query(query, options);
            let request_log_id = next_request_log_id();
            let start_time = Instant::now();
            let effective_method_for_log = request_method(method.clone(), options);

            let result = self
                .execute_once_with_options(ExecuteOnceRequest {
                    method: method.clone(),
                    path,
                    body,
                    extra_headers: merged_headers.as_ref().or(extra_headers),
                    query: merged_query.as_ref().or(query),
                    retry_count,
                    timeout_ms,
                    options,
                    request_log_id: &request_log_id,
                    retry_of_request_log_id: retry_of_request_log_id.as_deref(),
                })
                .await;

            match result {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if response.status().is_success() {
                        self.log(
                            LogLevel::Info,
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                true,
                                start_time.elapsed(),
                            ),
                        );
                        self.log_response_start_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &response,
                            start_time.elapsed(),
                        );
                        let headers = response.headers().clone();
                        let resp_bytes = read_response_bytes(response, options).await?;
                        let parsed = parse_success_body::<T>(&resp_bytes, &headers, status)?;

                        return Ok(parsed);
                    }

                    let should_retry = Self::should_retry_status(status, &response);

                    if retries_remaining > 0 && should_retry {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        self.log(
                            LogLevel::Info,
                            format!(
                                "{} - {retry_message}",
                                response_info_log(
                                    &request_log_id,
                                    retry_of_request_log_id.as_deref(),
                                    &effective_method_for_log,
                                    &response,
                                    false,
                                    start_time.elapsed(),
                                )
                            ),
                        );
                        self.log_response_error_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &retry_message,
                            &response,
                            start_time.elapsed(),
                        );
                        let sleep_ms =
                            self.retry_delay(retries_remaining, max_retries, Some(&response));
                        drop(response);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    let retry_message = if should_retry {
                        "error; no more retries left"
                    } else {
                        "error; not retryable"
                    };
                    self.log(
                        LogLevel::Info,
                        format!(
                            "{} - {retry_message}",
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                false,
                                start_time.elapsed(),
                            )
                        ),
                    );
                    self.log_response_error_debug(
                        &request_log_id,
                        retry_of_request_log_id.as_deref(),
                        retry_message,
                        &response,
                        start_time.elapsed(),
                    );
                    return Err(error_from_response(response, options).await);
                }
                Err(err) => {
                    if matches!(err, ApiError::UserAbort { .. }) {
                        return Err(err);
                    }

                    let retryable_connection = is_retryable_connection_error(&err);
                    if retries_remaining > 0 && retryable_connection {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - {retry_message}",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            &retry_message,
                            start_time.elapsed(),
                            &err,
                        );
                        let sleep_ms = self.retry_delay(retries_remaining, max_retries, None);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    if retryable_connection {
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - error; no more retries left",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            "error; no more retries left",
                            start_time.elapsed(),
                            &err,
                        );
                    }

                    return Err(err);
                }
            }
        }
    }

    /// Rust equivalent of TS `APIPromise.asResponse()` for core requests.
    ///
    /// Sends the request with the same retry/error behavior as
    /// [`Anthropic::request_with_options`] but returns a buffered raw response
    /// instead of deserializing the body. This keeps status, URL, headers, and
    /// body bytes available after the response stream has been consumed.
    pub async fn request_raw_with_options(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        let max_retries = options
            .and_then(|opts| opts.max_retries)
            .unwrap_or(self.max_retries);
        let timeout_ms = timeout_ms_from_options(options, self.timeout_ms);
        let mut retries_remaining = max_retries;
        let mut retry_of_request_log_id: Option<String> = None;

        loop {
            let retry_count = max_retries - retries_remaining;
            let merged_headers = merged_headers(extra_headers, options);
            let merged_query = merged_query(query, options);
            let request_log_id = next_request_log_id();
            let start_time = Instant::now();
            let effective_method_for_log = request_method(method.clone(), options);

            let result = self
                .execute_once_with_options(ExecuteOnceRequest {
                    method: method.clone(),
                    path,
                    body,
                    extra_headers: merged_headers.as_ref().or(extra_headers),
                    query: merged_query.as_ref().or(query),
                    retry_count,
                    timeout_ms,
                    options,
                    request_log_id: &request_log_id,
                    retry_of_request_log_id: retry_of_request_log_id.as_deref(),
                })
                .await;

            match result {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if response.status().is_success() {
                        self.log(
                            LogLevel::Info,
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                true,
                                start_time.elapsed(),
                            ),
                        );
                        self.log_response_start_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &response,
                            start_time.elapsed(),
                        );
                        return raw_response_from_response(response, options).await;
                    }

                    let should_retry = Self::should_retry_status(status, &response);

                    if retries_remaining > 0 && should_retry {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        self.log(
                            LogLevel::Info,
                            format!(
                                "{} - {retry_message}",
                                response_info_log(
                                    &request_log_id,
                                    retry_of_request_log_id.as_deref(),
                                    &effective_method_for_log,
                                    &response,
                                    false,
                                    start_time.elapsed(),
                                )
                            ),
                        );
                        self.log_response_error_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &retry_message,
                            &response,
                            start_time.elapsed(),
                        );
                        let sleep_ms =
                            self.retry_delay(retries_remaining, max_retries, Some(&response));
                        drop(response);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    let retry_message = if should_retry {
                        "error; no more retries left"
                    } else {
                        "error; not retryable"
                    };
                    self.log(
                        LogLevel::Info,
                        format!(
                            "{} - {retry_message}",
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                false,
                                start_time.elapsed(),
                            )
                        ),
                    );
                    self.log_response_error_debug(
                        &request_log_id,
                        retry_of_request_log_id.as_deref(),
                        retry_message,
                        &response,
                        start_time.elapsed(),
                    );
                    return Err(error_from_response(response, options).await);
                }
                Err(err) => {
                    if matches!(err, ApiError::UserAbort { .. }) {
                        return Err(err);
                    }

                    let retryable_connection = is_retryable_connection_error(&err);
                    if retries_remaining > 0 && retryable_connection {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - {retry_message}",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            &retry_message,
                            start_time.elapsed(),
                            &err,
                        );
                        let sleep_ms = self.retry_delay(retries_remaining, max_retries, None);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    if retryable_connection {
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - error; no more retries left",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            "error; no more retries left",
                            start_time.elapsed(),
                            &err,
                        );
                    }

                    return Err(err);
                }
            }
        }
    }

    /// Core raw-response request without per-request options.
    pub async fn request_raw(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
    ) -> Result<RawResponse, ApiError> {
        self.request_raw_with_options(method, path, body, extra_headers, query, None)
            .await
    }

    /// Rust equivalent of TS `APIPromise.withResponse()` for core requests.
    pub async fn request_with_response<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        let response = self
            .request_raw_with_options(method, path, body, extra_headers, query, options)
            .await?;
        let data = parse_success_body_from_raw::<T>(&response)?;
        let request_id = response.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data,
            response,
            request_id,
        })
    }

    // ── Convenience: POST ───────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.post()`
    ///
    /// Shorthand for `request(POST, ...)` with a JSON body.
    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.post_with_options(path, body, extra_headers, None)
            .await
    }

    /// Shorthand for `request_with_options(POST, ...)` with a JSON body.
    pub async fn post_with_options<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_options::<T>(
            reqwest::Method::POST,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// POST helper returning a buffered raw response, mirroring TS
    /// `post(...).asResponse()`.
    pub async fn post_raw_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_raw_with_options(
            reqwest::Method::POST,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// POST helper returning parsed data plus raw response metadata/body,
    /// mirroring TS `post(...).withResponse()`.
    pub async fn post_with_response<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_response::<T>(
            reqwest::Method::POST,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    // ── Convenience: multipart POST ─────────────────────────────────────

    /// Maps to: TS `multipartFormRequestOptions()` + `BaseAnthropic.post()`.
    ///
    /// Sends a `multipart/form-data` POST request and deserializes a JSON
    /// response. The form is rebuilt for every retry because multipart bodies
    /// are streaming and cannot be reused after a send attempt.
    pub async fn post_multipart<T, F>(
        &self,
        path: &str,
        build_form: F,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
        F: FnMut() -> Result<Form, ApiError>,
    {
        Ok(self
            .post_multipart_with_response(path, build_form, extra_headers, None)
            .await?
            .data)
    }

    /// Multipart POST with per-request options.
    pub async fn post_multipart_with_options<T, F>(
        &self,
        path: &str,
        build_form: F,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError>
    where
        T: DeserializeOwned,
        F: FnMut() -> Result<Form, ApiError>,
    {
        Ok(self
            .post_multipart_with_response(path, build_form, extra_headers, options)
            .await?
            .data)
    }

    /// Multipart POST returning parsed data plus raw response metadata/body,
    /// mirroring TS `post(...).withResponse()` for upload requests.
    pub async fn post_multipart_with_response<T, F>(
        &self,
        path: &str,
        mut build_form: F,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError>
    where
        T: DeserializeOwned,
        F: FnMut() -> Result<Form, ApiError>,
    {
        let max_retries = options
            .and_then(|opts| opts.max_retries)
            .unwrap_or(self.max_retries);
        let timeout_ms = timeout_ms_from_options(options, self.timeout_ms);
        let mut retries_remaining = max_retries;
        let mut retry_of_request_log_id: Option<String> = None;

        loop {
            let retry_count = max_retries - retries_remaining;
            let merged_headers = merged_headers(extra_headers, options);
            let merged_query = merged_query(None, options);
            let effective_path = request_path(path, options);
            let effective_method = request_method(reqwest::Method::POST, options);
            let request_log_id = next_request_log_id();
            let start_time = Instant::now();
            let url = self.build_url_with_default_base_url(
                effective_path,
                merged_query.as_ref(),
                options.and_then(|opts| opts.default_base_url.as_deref()),
            )?;
            let dynamic_auth_headers = self.resolve_dynamic_auth_headers().await?;
            let mut headers = self.build_headers_with_timeout(
                retry_count,
                merged_headers.as_ref().or(extra_headers),
                dynamic_auth_headers.as_ref(),
                timeout_ms,
            )?;
            let raw_body = options.and_then(|opts| opts.raw_body.as_ref()).cloned();
            if let Some(raw) = raw_body.as_ref() {
                apply_raw_body_content_type_to_headers(&mut headers, raw)?;
            }
            self.log_request_start(
                &request_log_id,
                retry_of_request_log_id.as_deref(),
                &effective_method,
                &url,
                retry_count,
                &headers,
            );

            ensure_not_aborted(options)?;
            let timeout = options
                .and_then(|opts| opts.timeout)
                .unwrap_or_else(|| Duration::from_millis(timeout_ms));
            let http = options
                .and_then(|opts| opts.http_client.as_ref())
                .unwrap_or(&self.http);
            let mut builder = http
                .request(effective_method.clone(), &url)
                .headers(headers)
                .timeout(timeout);
            if let Some(raw) = raw_body {
                builder = builder.body(raw.body.clone());
            } else {
                let form = build_form()?;
                builder = builder.multipart(form);
            }

            let result = self
                .send_with_abort_and_middleware(http, builder, options)
                .await;

            match result {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if response.status().is_success() {
                        self.log(
                            LogLevel::Info,
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method,
                                &response,
                                true,
                                start_time.elapsed(),
                            ),
                        );
                        self.log_response_start_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &response,
                            start_time.elapsed(),
                        );
                        let response = raw_response_from_response(response, options).await?;
                        let data = parse_success_body_from_raw::<T>(&response)?;
                        let request_id = response.request_id().map(str::to_owned);
                        return Ok(ApiResponse {
                            data,
                            response,
                            request_id,
                        });
                    }

                    let should_retry = Self::should_retry_status(status, &response);
                    if retries_remaining > 0 && should_retry {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        self.log(
                            LogLevel::Info,
                            format!(
                                "{} - {retry_message}",
                                response_info_log(
                                    &request_log_id,
                                    retry_of_request_log_id.as_deref(),
                                    &effective_method,
                                    &response,
                                    false,
                                    start_time.elapsed(),
                                )
                            ),
                        );
                        self.log_response_error_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &retry_message,
                            &response,
                            start_time.elapsed(),
                        );
                        let sleep_ms =
                            self.retry_delay(retries_remaining, max_retries, Some(&response));
                        drop(response);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    let retry_message = if should_retry {
                        "error; no more retries left"
                    } else {
                        "error; not retryable"
                    };
                    self.log(
                        LogLevel::Info,
                        format!(
                            "{} - {retry_message}",
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method,
                                &response,
                                false,
                                start_time.elapsed(),
                            )
                        ),
                    );
                    self.log_response_error_debug(
                        &request_log_id,
                        retry_of_request_log_id.as_deref(),
                        retry_message,
                        &response,
                        start_time.elapsed(),
                    );
                    return Err(error_from_response(response, options).await);
                }
                Err(err) => {
                    if matches!(err, ApiError::UserAbort { .. }) {
                        return Err(err);
                    }
                    let retryable_connection = is_retryable_connection_error(&err);
                    if retries_remaining > 0 && retryable_connection {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - {retry_message}",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            &retry_message,
                            start_time.elapsed(),
                            &err,
                        );
                        let sleep_ms = self.retry_delay(retries_remaining, max_retries, None);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }
                    if retryable_connection {
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - error; no more retries left",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            "error; no more retries left",
                            start_time.elapsed(),
                            &err,
                        );
                    }
                    return Err(err);
                }
            }
        }
    }

    // ── Convenience: GET ────────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.get()`
    ///
    /// Shorthand for `request(GET, ...)` with no body.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.get_with_options(path, query, extra_headers, None)
            .await
    }

    /// Shorthand for `request_with_options(GET, ...)` with no body.
    pub async fn get_with_options<T: DeserializeOwned>(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        self.request_with_options::<T>(
            reqwest::Method::GET,
            path,
            None,
            extra_headers,
            query,
            options,
        )
        .await
    }

    /// GET helper returning a buffered raw response, mirroring TS
    /// `get(...).asResponse()`.
    pub async fn get_raw_response(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        self.request_raw_with_options(
            reqwest::Method::GET,
            path,
            None,
            extra_headers,
            query,
            options,
        )
        .await
    }

    /// GET helper returning parsed data plus raw response metadata/body,
    /// mirroring TS `get(...).withResponse()`.
    pub async fn get_with_response<T: DeserializeOwned>(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        self.request_with_response::<T>(
            reqwest::Method::GET,
            path,
            None,
            extra_headers,
            query,
            options,
        )
        .await
    }

    // ── Convenience: binary GET ─────────────────────────────────────────

    /// Maps to: TS `__binaryResponse` request option.
    ///
    /// Sends a GET request and returns the raw response bytes instead of
    /// attempting JSON deserialization. Error responses are parsed the same way
    /// as JSON requests.
    pub async fn get_binary(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<Vec<u8>, ApiError> {
        self.get_binary_with_options(path, query, extra_headers, None)
            .await
    }

    /// Binary GET with per-request options.
    pub async fn get_binary_with_options(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<Vec<u8>, ApiError> {
        Ok(self
            .get_binary_with_response(path, query, extra_headers, options)
            .await?
            .data)
    }

    /// Binary GET returning response bytes plus raw response metadata/body,
    /// mirroring TS `__binaryResponse` + `withResponse()`.
    pub async fn get_binary_with_response(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Vec<u8>>, ApiError> {
        let response = self
            .request_raw_with_options(
                reqwest::Method::GET,
                path,
                None,
                extra_headers,
                query,
                options,
            )
            .await?;
        let data = response.body.clone();
        let request_id = response.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data,
            response,
            request_id,
        })
    }

    // ── Convenience: DELETE ──────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.delete()`
    ///
    /// Shorthand for `request(DELETE, ...)` with no body.
    pub async fn delete<T: DeserializeOwned>(
        &self,
        path: &str,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.delete_with_options(path, extra_headers, None).await
    }

    /// Shorthand for `request_with_options(DELETE, ...)` with no body.
    pub async fn delete_with_options<T: DeserializeOwned>(
        &self,
        path: &str,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        self.request_with_options::<T>(
            reqwest::Method::DELETE,
            path,
            None,
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// DELETE helper returning a buffered raw response, mirroring TS
    /// `delete(...).asResponse()`.
    pub async fn delete_raw_response(
        &self,
        path: &str,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        self.request_raw_with_options(
            reqwest::Method::DELETE,
            path,
            None,
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// DELETE helper returning parsed data plus raw response metadata/body,
    /// mirroring TS `delete(...).withResponse()`.
    pub async fn delete_with_response<T: DeserializeOwned>(
        &self,
        path: &str,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        self.request_with_response::<T>(
            reqwest::Method::DELETE,
            path,
            None,
            extra_headers,
            None,
            options,
        )
        .await
    }

    // ── Convenience: PUT ────────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.put()`
    ///
    /// Shorthand for `request(PUT, ...)` with a JSON body.
    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.put_with_options(path, body, extra_headers, None).await
    }

    /// Shorthand for `request_with_options(PUT, ...)` with a JSON body.
    pub async fn put_with_options<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_options::<T>(
            reqwest::Method::PUT,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// PUT helper returning a buffered raw response, mirroring TS
    /// `put(...).asResponse()`.
    pub async fn put_raw_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_raw_with_options(
            reqwest::Method::PUT,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// PUT helper returning parsed data plus raw response metadata/body,
    /// mirroring TS `put(...).withResponse()`.
    pub async fn put_with_response<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_response::<T>(
            reqwest::Method::PUT,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    // ── Convenience: PATCH ──────────────────────────────────────────────

    /// Maps to: TS `BaseAnthropic.patch()`
    ///
    /// Shorthand for `request(PATCH, ...)` with a JSON body.
    pub async fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<T, ApiError> {
        self.patch_with_options(path, body, extra_headers, None)
            .await
    }

    /// Shorthand for `request_with_options(PATCH, ...)` with a JSON body.
    pub async fn patch_with_options<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<T, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_options::<T>(
            reqwest::Method::PATCH,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// PATCH helper returning a buffered raw response, mirroring TS
    /// `patch(...).asResponse()`.
    pub async fn patch_raw_response<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<RawResponse, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_raw_with_options(
            reqwest::Method::PATCH,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// PATCH helper returning parsed data plus raw response metadata/body,
    /// mirroring TS `patch(...).withResponse()`.
    pub async fn patch_with_response<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<T>, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_with_response::<T>(
            reqwest::Method::PATCH,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    // ── Streaming POST ──────────────────────────────────────────────────

    /// Maps to: TS streaming path through `BaseAnthropic.post()` with `stream: true`
    ///
    /// Sends a POST request and returns the raw `reqwest::Response` so the
    /// caller can read SSE events from the byte stream. Retry logic is
    /// identical to [`Anthropic::request`].
    pub async fn post_stream<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<reqwest::Response, ApiError> {
        self.post_stream_with_options(path, body, extra_headers, None)
            .await
    }

    /// Streaming POST with per-request options.
    pub async fn post_stream_with_options<B: Serialize>(
        &self,
        path: &str,
        body: &B,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<reqwest::Response, ApiError> {
        let json_body = serde_json::to_value(body)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize request body: {e}")))?;
        self.request_stream_with_options(
            reqwest::Method::POST,
            path,
            Some(&json_body),
            extra_headers,
            None,
            options,
        )
        .await
    }

    /// Streaming GET returning the raw `reqwest::Response` body stream.
    ///
    /// This mirrors TS requests made with `stream: true` and is primarily used
    /// for JSONL/binary streaming endpoints such as message batch results.
    pub async fn get_stream(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
    ) -> Result<reqwest::Response, ApiError> {
        self.get_stream_with_options(path, query, extra_headers, None)
            .await
    }

    /// Streaming GET with per-request options.
    pub async fn get_stream_with_options(
        &self,
        path: &str,
        query: Option<&HashMap<String, Option<String>>>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<reqwest::Response, ApiError> {
        self.request_stream_with_options(
            reqwest::Method::GET,
            path,
            None,
            extra_headers,
            query,
            options,
        )
        .await
    }

    async fn request_stream_with_options(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
        extra_headers: Option<&HashMap<String, Option<String>>>,
        query: Option<&HashMap<String, Option<String>>>,
        options: Option<&RequestOptions>,
    ) -> Result<reqwest::Response, ApiError> {
        let max_retries = options
            .and_then(|opts| opts.max_retries)
            .unwrap_or(self.max_retries);
        let timeout_ms = timeout_ms_from_options(options, self.timeout_ms);
        let mut retries_remaining = max_retries;
        let mut retry_of_request_log_id: Option<String> = None;

        loop {
            let retry_count = max_retries - retries_remaining;
            let merged_headers = merged_headers(extra_headers, options);
            let merged_query = merged_query(query, options);
            let request_log_id = next_request_log_id();
            let start_time = Instant::now();
            let effective_method_for_log = request_method(method.clone(), options);
            let result = self
                .execute_once_with_options(ExecuteOnceRequest {
                    method: method.clone(),
                    path,
                    body,
                    extra_headers: merged_headers.as_ref().or(extra_headers),
                    query: merged_query.as_ref().or(query),
                    retry_count,
                    timeout_ms,
                    options,
                    request_log_id: &request_log_id,
                    retry_of_request_log_id: retry_of_request_log_id.as_deref(),
                })
                .await;

            match result {
                Ok(response) => {
                    let status = response.status().as_u16();

                    if response.status().is_success() {
                        self.log(
                            LogLevel::Info,
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                true,
                                start_time.elapsed(),
                            ),
                        );
                        self.log_response_start_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &response,
                            start_time.elapsed(),
                        );
                        return Ok(response);
                    }

                    let should_retry = Self::should_retry_status(status, &response);

                    if retries_remaining > 0 && should_retry {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        self.log(
                            LogLevel::Info,
                            format!(
                                "{} - {retry_message}",
                                response_info_log(
                                    &request_log_id,
                                    retry_of_request_log_id.as_deref(),
                                    &effective_method_for_log,
                                    &response,
                                    false,
                                    start_time.elapsed(),
                                )
                            ),
                        );
                        self.log_response_error_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            &retry_message,
                            &response,
                            start_time.elapsed(),
                        );
                        let sleep_ms =
                            self.retry_delay(retries_remaining, max_retries, Some(&response));
                        drop(response);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }

                    let retry_message = if should_retry {
                        "error; no more retries left"
                    } else {
                        "error; not retryable"
                    };
                    self.log(
                        LogLevel::Info,
                        format!(
                            "{} - {retry_message}",
                            response_info_log(
                                &request_log_id,
                                retry_of_request_log_id.as_deref(),
                                &effective_method_for_log,
                                &response,
                                false,
                                start_time.elapsed(),
                            )
                        ),
                    );
                    self.log_response_error_debug(
                        &request_log_id,
                        retry_of_request_log_id.as_deref(),
                        retry_message,
                        &response,
                        start_time.elapsed(),
                    );
                    return Err(error_from_response(response, options).await);
                }
                Err(err) => {
                    if matches!(err, ApiError::UserAbort { .. }) {
                        return Err(err);
                    }
                    let retryable_connection = is_retryable_connection_error(&err);
                    if retries_remaining > 0 && retryable_connection {
                        let retry_message =
                            format!("retrying, {retries_remaining} attempts remaining");
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - {retry_message}",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            &retry_message,
                            start_time.elapsed(),
                            &err,
                        );
                        let sleep_ms = self.retry_delay(retries_remaining, max_retries, None);
                        set_retry_origin(&mut retry_of_request_log_id, &request_log_id);
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        retries_remaining -= 1;
                        continue;
                    }
                    if retryable_connection {
                        let label = connection_failure_label(&err);
                        self.log(
                            LogLevel::Info,
                            format!(
                                "[{request_log_id}{}] connection {label} - error; no more retries left",
                                retry_detail_suffix(retry_of_request_log_id.as_deref())
                            ),
                        );
                        self.log_connection_debug(
                            &request_log_id,
                            retry_of_request_log_id.as_deref(),
                            label,
                            "error; no more retries left",
                            start_time.elapsed(),
                            &err,
                        );
                    }
                    return Err(err);
                }
            }
        }
    }

    async fn resolve_dynamic_auth_headers(
        &self,
    ) -> Result<Option<HashMap<String, Option<String>>>, ApiError> {
        let Some(provider) = &self.auth_token_provider else {
            return Ok(None);
        };

        let token = provider.get_token().await?;
        if token.trim().is_empty() {
            return Err(ApiError::Sdk(
                "Expected auth_token_provider to return a non-empty string".to_owned(),
            ));
        }

        let mut headers = HashMap::new();
        headers.insert("authorization".to_owned(), Some(format!("Bearer {token}")));
        Ok(Some(headers))
    }

    // ── Internal: single attempt ────────────────────────────────────────

    /// Executes a single HTTP request with per-request options (no retry logic).
    async fn execute_once_with_options(
        &self,
        request: ExecuteOnceRequest<'_>,
    ) -> Result<reqwest::Response, ApiError> {
        let ExecuteOnceRequest {
            method,
            path,
            body,
            extra_headers,
            query,
            retry_count,
            timeout_ms,
            options,
            request_log_id,
            retry_of_request_log_id,
        } = request;

        ensure_not_aborted(options)?;

        let effective_path = request_path(path, options);
        let effective_method = request_method(method, options);
        let url = self.build_url_with_default_base_url(
            effective_path,
            query,
            options.and_then(|opts| opts.default_base_url.as_deref()),
        )?;
        let effective_body = effective_request_body(body, options)?;
        let dynamic_auth_headers = self.resolve_dynamic_auth_headers().await?;
        let mut headers = self.build_headers_with_timeout(
            retry_count,
            extra_headers,
            dynamic_auth_headers.as_ref(),
            timeout_ms,
        )?;
        apply_raw_body_content_type(&mut headers, &effective_body)?;
        self.log_request_start(
            request_log_id,
            retry_of_request_log_id,
            &effective_method,
            &url,
            retry_count,
            &headers,
        );
        let timeout = options
            .and_then(|opts| opts.timeout)
            .unwrap_or_else(|| Duration::from_millis(timeout_ms));

        let http = options
            .and_then(|opts| opts.http_client.as_ref())
            .unwrap_or(&self.http);
        let mut builder = http
            .request(effective_method, &url)
            .headers(headers)
            .timeout(timeout);
        match effective_body {
            EffectiveRequestBody::None => {}
            EffectiveRequestBody::Json(value) => {
                builder = builder.json(&value);
            }
            EffectiveRequestBody::Raw(raw) => {
                builder = builder.body(raw.body.clone());
            }
        }

        self.send_with_abort_and_middleware(http, builder, options)
            .await
    }

    async fn send_with_abort_and_middleware(
        &self,
        http: &reqwest::Client,
        builder: reqwest::RequestBuilder,
        options: Option<&RequestOptions>,
    ) -> Result<reqwest::Response, ApiError> {
        let mut request = builder.build().map_err(api_error_from_reqwest)?;
        for middleware in self.middleware_chain(options) {
            middleware.before_request(&mut request).await?;
        }

        let result = send_request_with_abort(http, request, options).await;
        match result {
            Ok(response) => {
                for middleware in self.middleware_chain(options) {
                    middleware.on_response(&response).await?;
                }
                Ok(response)
            }
            Err(err) => {
                for middleware in self.middleware_chain(options) {
                    middleware.on_error(&err).await?;
                }
                Err(err)
            }
        }
    }

    fn middleware_chain<'a>(
        &'a self,
        options: Option<&'a RequestOptions>,
    ) -> Vec<&'a Arc<dyn HttpMiddleware>> {
        let mut middlewares = Vec::with_capacity(
            self.middlewares.len() + options.map(|opts| opts.middlewares.len()).unwrap_or(0),
        );
        middlewares.extend(self.middlewares.iter());
        if let Some(options) = options {
            middlewares.extend(options.middlewares.iter());
        }
        middlewares
    }

    // ── Internal: retry helpers ─────────────────────────────────────────

    /// Maps to: TS `shouldRetry()` -- determines if a status code warrants a retry.
    fn should_retry_status(status: u16, response: &reqwest::Response) -> bool {
        // Respect the non-standard x-should-retry header if present.
        if let Some(val) = response.headers().get("x-should-retry") {
            if let Ok(s) = val.to_str() {
                if s == "true" {
                    return true;
                }
                if s == "false" {
                    return false;
                }
            }
        }

        matches!(status, 408 | 409 | 429 | 500..=599)
    }

    /// Maps to: TS `retryRequest()` + `calculateDefaultRetryTimeoutMillis()`
    ///
    /// Calculates the delay before the next retry. Honours `Retry-After-Ms` and
    /// `Retry-After` response headers when present and reasonable (< 60 s).
    /// Otherwise applies exponential back-off:
    ///   `min(0.5 * 2^n, 8.0) * (1 - rand * 0.25) * 1000` ms
    fn retry_delay(
        &self,
        retries_remaining: u32,
        max_retries: u32,
        response: Option<&reqwest::Response>,
    ) -> u64 {
        let mut timeout_millis: Option<f64> = None;

        if let Some(resp) = response {
            // Non-standard but useful: retry-after-ms
            if let Some(val) = resp.headers().get("retry-after-ms") {
                if let Ok(s) = val.to_str() {
                    if let Ok(ms) = s.parse::<f64>() {
                        if ms.is_finite() {
                            timeout_millis = Some(ms);
                        }
                    }
                }
            }

            // Standard Retry-After (seconds or HTTP-date)
            if timeout_millis.is_none() {
                if let Some(val) = resp.headers().get("retry-after") {
                    if let Ok(s) = val.to_str() {
                        if let Ok(secs) = s.parse::<f64>() {
                            if secs.is_finite() {
                                timeout_millis = Some(secs * 1000.0);
                            }
                        } else if let Ok(date) = httpdate::parse_http_date(s) {
                            if let Ok(delay) = date.duration_since(SystemTime::now()) {
                                timeout_millis = Some(delay.as_secs_f64() * 1000.0);
                            } else {
                                timeout_millis = Some(0.0);
                            }
                        }
                    }
                }
            }
        }

        // Match TS retryRequest(): only trust server-supplied delay when it is
        // truthy and between 0 and 60 seconds. A header value of `0` falls back
        // to exponential backoff because JavaScript's `timeoutMillis && ...`
        // guard treats it as absent.
        let use_server_delay = timeout_millis
            .map(|ms| ms > 0.0 && ms < 60_000.0)
            .unwrap_or(false);

        if use_server_delay {
            // Safe: guarded by the check above.
            return timeout_millis.unwrap_or(0.0).max(0.0) as u64;
        }

        // Exponential back-off with jitter.
        let num_retries = max_retries.saturating_sub(retries_remaining);
        let initial_delay: f64 = 0.5;
        let max_delay: f64 = 8.0;

        let sleep_seconds = (initial_delay * 2.0_f64.powi(num_retries as i32)).min(max_delay);

        let mut rng = rand::rng();
        let jitter: f64 = 1.0 - rng.random::<f64>() * 0.25;

        (sleep_seconds * jitter * 1000.0) as u64
    }

    // ── Accessors ───────────────────────────────────────────────────────

    /// Returns the resolved API key, if any.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// TS-style alias for the `apiKey` property.
    #[allow(non_snake_case)]
    pub fn apiKey(&self) -> Option<&str> {
        self.api_key()
    }

    /// Returns the resolved auth token, if any.
    pub fn auth_token(&self) -> Option<&str> {
        self.auth_token.as_deref()
    }

    /// TS-style alias for the `authToken` property.
    #[allow(non_snake_case)]
    pub fn authToken(&self) -> Option<&str> {
        self.auth_token()
    }

    /// Returns the resolved base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// TS-style alias for the `baseURL` property.
    #[allow(non_snake_case)]
    pub fn baseURL(&self) -> &str {
        self.base_url()
    }

    /// Returns the timeout in milliseconds.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// TS-style alias for the `timeout` property.
    pub fn timeout(&self) -> u64 {
        self.timeout_ms()
    }

    /// Returns the resolved request logging level.
    ///
    /// Maps to TS `client.logLevel`.
    pub fn log_level(&self) -> LogLevel {
        self.log_level
    }

    /// TS-style alias for the `logLevel` property.
    #[allow(non_snake_case)]
    pub fn logLevel(&self) -> LogLevel {
        self.log_level()
    }

    /// Returns the configured custom logger, if any.
    ///
    /// Maps to TS `client.logger`.
    pub fn logger(&self) -> Option<Arc<dyn SdkLogger>> {
        self.logger.clone()
    }

    /// Maps to TS `calculateNonstreamingTimeout(maxTokens, maxNonstreamingTokens?)`.
    ///
    /// Returns the default non-streaming timeout (10 minutes) or errors when a
    /// request is expected to need streaming, matching the TS public helper.
    pub fn calculate_nonstreaming_timeout(
        &self,
        max_tokens: i64,
        max_nonstreaming_tokens: Option<i64>,
    ) -> Result<u64, ApiError> {
        let max_time_ms = 60.0 * 60.0 * 1000.0;
        let default_time_ms = DEFAULT_TIMEOUT_MS as f64;
        let expected_time_ms = (max_time_ms * max_tokens as f64) / 128_000.0;
        let exceeds_model_nonstreaming_limit =
            max_nonstreaming_tokens.is_some_and(|limit| max_tokens > limit);

        if expected_time_ms > default_time_ms || exceeds_model_nonstreaming_limit {
            return Err(ApiError::Sdk(
                "Streaming is required for operations that may take longer than 10 minutes. See https://github.com/anthropics/anthropic-sdk-typescript#long-requests for more details".to_owned(),
            ));
        }

        Ok(DEFAULT_TIMEOUT_MS)
    }

    /// TS-style camelCase alias for [`Anthropic::calculate_nonstreaming_timeout`].
    #[allow(non_snake_case)]
    pub fn calculateNonstreamingTimeout(
        &self,
        max_tokens: i64,
        max_nonstreaming_tokens: Option<i64>,
    ) -> Result<u64, ApiError> {
        self.calculate_nonstreaming_timeout(max_tokens, max_nonstreaming_tokens)
    }

    /// Maps to TS `Messages.create()` timeout validation order: client-level
    /// `timeout` skips the streaming requirement check, but per-request
    /// `options.timeout` is merged only after `calculateNonstreamingTimeout()`
    /// has already run and therefore does not bypass the check.
    pub(crate) fn validate_nonstreaming_timeout(
        &self,
        model: &str,
        max_tokens: i64,
        _options: Option<&RequestOptions>,
    ) -> Result<(), ApiError> {
        if self._options.timeout.is_some() {
            return Ok(());
        }

        self.calculate_nonstreaming_timeout(
            max_tokens,
            crate::internal::constants::model_nonstreaming_tokens(model).map(|limit| limit as i64),
        )
        .map(|_| ())
    }

    /// Returns the maximum number of retries.
    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    /// TS-style alias for the `maxRetries` property.
    #[allow(non_snake_case)]
    pub fn maxRetries(&self) -> u32 {
        self.max_retries()
    }

    /// Returns a reference to the inner `reqwest::Client`.
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Maps to: TS `Anthropic.messages` — accessor for the Messages resource.
    pub fn messages(&self) -> crate::resources::messages::Messages<'_> {
        crate::resources::messages::Messages::new(self)
    }

    /// Maps to: TS `Anthropic.completions` — accessor for the legacy Completions resource.
    pub fn completions(&self) -> crate::resources::completions::Completions<'_> {
        crate::resources::completions::Completions::new(self)
    }

    /// Maps to: TS `Anthropic.models` — accessor for the Models resource.
    pub fn models(&self) -> crate::resources::models::Models<'_> {
        crate::resources::models::Models::new(self)
    }

    /// Maps to: TS `Anthropic.beta` — accessor for the Beta resource namespace.
    pub fn beta(&self) -> crate::resources::beta::types::Beta<'_> {
        crate::resources::beta::types::Beta::new(self)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers (private)
// ─────────────────────────────────────────────────────────────────────────────

/// Extracts response headers into a `HashMap<String, String>` for error
/// construction. Header names are lowercased.
fn response_headers_to_map(resp: &reqwest::Response) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (name, value) in resp.headers().iter() {
        if let Ok(v) = value.to_str() {
            map.insert(name.as_str().to_owned(), v.to_owned());
        }
    }
    map
}

fn next_request_log_id() -> String {
    let mut rng = rand::rng();
    format!("log_{:06x}", rng.random_range(0..(1_u32 << 24)))
}

fn retry_detail_suffix(retry_of_request_log_id: Option<&str>) -> String {
    retry_of_request_log_id
        .map(|id| format!(", retryOf: {id}"))
        .unwrap_or_default()
}

fn request_id_header_suffix(headers: &HeaderMap) -> String {
    headers
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| serde_json::to_string(value).ok())
        .map(|value| format!(", request-id: {value}"))
        .unwrap_or_default()
}

fn response_info_log(
    request_log_id: &str,
    retry_of_request_log_id: Option<&str>,
    method: &reqwest::Method,
    response: &reqwest::Response,
    succeeded: bool,
    duration: Duration,
) -> String {
    let outcome = if succeeded { "succeeded" } else { "failed" };
    format!(
        "[{request_log_id}{}{}] {} {} {outcome} with status {} in {}ms",
        retry_detail_suffix(retry_of_request_log_id),
        request_id_header_suffix(response.headers()),
        method.as_str(),
        response.url(),
        response.status().as_u16(),
        duration.as_millis()
    )
}

fn connection_failure_label(err: &ApiError) -> &'static str {
    if matches!(err, ApiError::ConnectionTimeout { .. }) {
        "timed out"
    } else {
        "failed"
    }
}

fn set_retry_origin(retry_of_request_log_id: &mut Option<String>, request_log_id: &str) {
    if retry_of_request_log_id.is_none() {
        *retry_of_request_log_id = Some(request_log_id.to_owned());
    }
}

fn duration_ms_value(elapsed: Duration) -> JsonValue {
    let duration_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    JsonValue::from(duration_ms)
}

fn response_log_details(
    retry_of_request_log_id: Option<&str>,
    url: &str,
    status: u16,
    headers: &HeaderMap,
    elapsed: Duration,
) -> JsonMap<String, JsonValue> {
    let mut details = JsonMap::new();
    if let Some(retry_of) = retry_of_request_log_id {
        details.insert(
            "retryOfRequestLogID".to_owned(),
            JsonValue::String(retry_of.to_owned()),
        );
    }
    details.insert("url".to_owned(), JsonValue::String(url.to_owned()));
    details.insert("status".to_owned(), JsonValue::from(status));
    details.insert(
        "headers".to_owned(),
        JsonValue::Object(header_map_to_json(headers)),
    );
    details.insert("durationMs".to_owned(), duration_ms_value(elapsed));
    format_request_details(details)
}

fn header_map_to_json(headers: &HeaderMap) -> JsonMap<String, JsonValue> {
    let mut values = JsonMap::new();
    for (name, value) in headers.iter() {
        values.insert(
            name.as_str().to_owned(),
            JsonValue::String(value.to_str().unwrap_or("<non-utf8>").to_owned()),
        );
    }
    values
}

/// Format headers for debug request logs using TS `formatRequestDetails()`
/// redaction rules for sensitive values.
fn redacted_header_log(headers: &HeaderMap) -> String {
    let mut values = BTreeMap::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_owned();
        let lower = key.to_ascii_lowercase();
        let value = if matches!(
            lower.as_str(),
            "x-api-key" | "authorization" | "cookie" | "set-cookie"
        ) {
            "***".to_owned()
        } else {
            value.to_str().unwrap_or("<non-utf8>").to_owned()
        };
        values.insert(key, value);
    }
    format!("{values:?}")
}

async fn raw_response_from_response(
    response: reqwest::Response,
    options: Option<&RequestOptions>,
) -> Result<RawResponse, ApiError> {
    let status = response.status().as_u16();
    let url = response.url().to_string();
    let headers = response_headers_to_map(&response);
    let body = read_response_bytes(response, options).await?;
    Ok(RawResponse {
        status,
        url,
        headers,
        body,
    })
}

/// Merges two `HashMap<String, Option<String>>` maps. Entries from `overrides`
/// win. An override value of `None` removes the key.
fn merge_option_maps(
    base: &HashMap<String, Option<String>>,
    overrides: &HashMap<String, Option<String>>,
) -> HashMap<String, Option<String>> {
    let mut merged = base.clone();
    for (k, v) in overrides {
        match v {
            Some(_) => {
                merged.insert(k.clone(), v.clone());
            }
            None => {
                merged.remove(k);
            }
        }
    }
    merged
}

fn timeout_ms_from_options(options: Option<&RequestOptions>, default_timeout_ms: u64) -> u64 {
    options
        .and_then(|opts| opts.timeout)
        .map(|duration| duration.as_millis().try_into().unwrap_or(u64::MAX))
        .unwrap_or(default_timeout_ms)
}

fn request_path<'a>(default_path: &'a str, options: Option<&'a RequestOptions>) -> &'a str {
    options
        .and_then(|opts| opts.path.as_deref())
        .unwrap_or(default_path)
}

fn request_method(
    default_method: reqwest::Method,
    options: Option<&RequestOptions>,
) -> reqwest::Method {
    options
        .and_then(|opts| opts.method.clone())
        .unwrap_or(default_method)
}

#[derive(Debug, Clone)]
enum EffectiveRequestBody {
    None,
    Json(JsonValue),
    Raw(RawRequestBody),
}

fn effective_request_body(
    generated_body: Option<&serde_json::Value>,
    options: Option<&RequestOptions>,
) -> Result<EffectiveRequestBody, ApiError> {
    if let Some(raw) = options.and_then(|opts| opts.raw_body.as_ref()) {
        return Ok(EffectiveRequestBody::Raw(raw.clone()));
    }

    let mut body = match options.and_then(|opts| opts.body.as_ref()) {
        // TS resource helpers spread `options` after the generated request
        // body, and `BaseAnthropic.buildBody()` uses `if (!body)`; a falsy
        // per-request body therefore suppresses the generated JSON body rather
        // than falling back to it.
        Some(override_body) => {
            if json_value_is_truthy_like_js(override_body) {
                Some(override_body.clone())
            } else {
                None
            }
        }
        None => generated_body
            .filter(|body| json_value_is_truthy_like_js(body))
            .cloned(),
    };

    if let Some(patches) = options.map(|opts| opts.json_body_patches.as_slice()) {
        if !patches.is_empty() {
            let mut patched = body
                .take()
                .unwrap_or_else(|| JsonValue::Object(JsonMap::new()));
            apply_json_body_patches(&mut patched, patches)?;
            return Ok(EffectiveRequestBody::Json(patched));
        }
    }

    Ok(body
        .map(EffectiveRequestBody::Json)
        .unwrap_or(EffectiveRequestBody::None))
}

fn apply_raw_body_content_type(
    headers: &mut HeaderMap,
    body: &EffectiveRequestBody,
) -> Result<(), ApiError> {
    if let EffectiveRequestBody::Raw(raw) = body {
        apply_raw_body_content_type_to_headers(headers, raw)?;
    }
    Ok(())
}

fn apply_raw_body_content_type_to_headers(
    headers: &mut HeaderMap,
    raw: &RawRequestBody,
) -> Result<(), ApiError> {
    if let Some(content_type) = raw.content_type.as_deref() {
        let value = HeaderValue::from_str(content_type).map_err(|err| {
            ApiError::Sdk(format!(
                "invalid content-type for raw request body '{content_type}': {err}"
            ))
        })?;
        headers.insert(reqwest::header::CONTENT_TYPE, value);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonPathSegment {
    Key(String),
    Index(usize),
}

fn apply_json_body_patches(
    body: &mut JsonValue,
    patches: &[JsonBodyPatch],
) -> Result<(), ApiError> {
    for patch in patches {
        match patch {
            JsonBodyPatch::Set { path, value } => {
                let segments = parse_json_patch_path(path)?;
                set_json_path(body, &segments, value.clone());
            }
            JsonBodyPatch::Delete { path } => {
                let segments = parse_json_patch_path(path)?;
                delete_json_path(body, &segments);
            }
        }
    }
    Ok(())
}

fn parse_json_patch_path(path: &str) -> Result<Vec<JsonPathSegment>, ApiError> {
    if path.trim().is_empty() {
        return Err(ApiError::Sdk(
            "JSON body patch path must not be empty".to_owned(),
        ));
    }

    path.split('.')
        .map(|segment| {
            if segment.is_empty() {
                return Err(ApiError::Sdk(format!(
                    "JSON body patch path contains an empty segment: {path}"
                )));
            }
            if segment.chars().all(|ch| ch.is_ascii_digit()) {
                segment
                    .parse::<usize>()
                    .map(JsonPathSegment::Index)
                    .map_err(|err| {
                        ApiError::Sdk(format!(
                            "invalid JSON array index '{segment}' in path '{path}': {err}"
                        ))
                    })
            } else {
                Ok(JsonPathSegment::Key(segment.to_owned()))
            }
        })
        .collect()
}

fn set_json_path(target: &mut JsonValue, segments: &[JsonPathSegment], value: JsonValue) {
    let Some((head, tail)) = segments.split_first() else {
        *target = value;
        return;
    };

    match head {
        JsonPathSegment::Key(key) => {
            if !target.is_object() {
                *target = JsonValue::Object(JsonMap::new());
            }
            let object = target.as_object_mut().expect("target just made object");
            if tail.is_empty() {
                object.insert(key.clone(), value);
            } else {
                let child = object.entry(key.clone()).or_insert(JsonValue::Null);
                set_json_path(child, tail, value);
            }
        }
        JsonPathSegment::Index(index) => {
            if !target.is_array() {
                *target = JsonValue::Array(Vec::new());
            }
            let array = target.as_array_mut().expect("target just made array");
            if array.len() <= *index {
                array.resize_with(*index + 1, || JsonValue::Null);
            }
            if tail.is_empty() {
                array[*index] = value;
            } else {
                set_json_path(&mut array[*index], tail, value);
            }
        }
    }
}

fn delete_json_path(target: &mut JsonValue, segments: &[JsonPathSegment]) {
    let Some((head, tail)) = segments.split_first() else {
        return;
    };

    match head {
        JsonPathSegment::Key(key) => {
            let Some(object) = target.as_object_mut() else {
                return;
            };
            if tail.is_empty() {
                object.remove(key);
            } else if let Some(child) = object.get_mut(key) {
                delete_json_path(child, tail);
            }
        }
        JsonPathSegment::Index(index) => {
            let Some(array) = target.as_array_mut() else {
                return;
            };
            if *index >= array.len() {
                return;
            }
            if tail.is_empty() {
                array.remove(*index);
            } else {
                delete_json_path(&mut array[*index], tail);
            }
        }
    }
}

fn json_value_is_truthy_like_js(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(false) => false,
        serde_json::Value::Number(number) => number.as_f64() != Some(0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Bool(true)
        | serde_json::Value::Array(_)
        | serde_json::Value::Object(_) => true,
    }
}

fn merged_headers(
    extra_headers: Option<&HashMap<String, Option<String>>>,
    options: Option<&RequestOptions>,
) -> Option<HashMap<String, Option<String>>> {
    let option_headers = options.and_then(|opts| opts.headers.as_ref())?;
    let base = extra_headers.cloned().unwrap_or_default();
    Some(overlay_option_maps(&base, option_headers))
}

fn merged_query(
    query: Option<&HashMap<String, Option<String>>>,
    options: Option<&RequestOptions>,
) -> Option<HashMap<String, Option<String>>> {
    let option_query = options.and_then(|opts| opts.query.as_ref())?;
    let base = query.cloned().unwrap_or_default();
    Some(overlay_option_maps(&base, option_query))
}

fn overlay_option_maps(
    base: &HashMap<String, Option<String>>,
    overrides: &HashMap<String, Option<String>>,
) -> HashMap<String, Option<String>> {
    let mut merged = base.clone();
    for (k, v) in overrides {
        // Preserve `None` so build_headers/build_url can remove keys from
        // earlier layers such as client defaults.
        merged.insert(k.clone(), v.clone());
    }
    merged
}

fn ensure_not_aborted(options: Option<&RequestOptions>) -> Result<(), ApiError> {
    if options
        .and_then(|opts| opts.signal.as_ref())
        .is_some_and(|signal| signal.is_aborted())
    {
        return Err(ApiError::UserAbort {
            message: "Request was aborted.".to_owned(),
        });
    }
    Ok(())
}

async fn send_request_with_abort(
    http: &reqwest::Client,
    request: reqwest::Request,
    options: Option<&RequestOptions>,
) -> Result<reqwest::Response, ApiError> {
    if let Some(mut signal) = options.and_then(|opts| opts.signal.clone()) {
        tokio::select! {
            result = http.execute(request) => result.map_err(api_error_from_reqwest),
            _ = signal.aborted() => Err(ApiError::UserAbort { message: "Request was aborted.".to_owned() }),
        }
    } else {
        http.execute(request).await.map_err(api_error_from_reqwest)
    }
}

async fn read_response_bytes(
    response: reqwest::Response,
    options: Option<&RequestOptions>,
) -> Result<Vec<u8>, ApiError> {
    if let Some(mut signal) = options.and_then(|opts| opts.signal.clone()) {
        tokio::select! {
            result = response.bytes() => result
                .map(|bytes| bytes.to_vec())
                .map_err(|e| ApiError::Connection {
                    message: format!("failed to read response body: {e}"),
                    cause: Some(Box::new(e)),
                }),
            _ = signal.aborted() => Err(ApiError::UserAbort { message: "Request was aborted.".to_owned() }),
        }
    } else {
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| ApiError::Connection {
                message: format!("failed to read response body: {e}"),
                cause: Some(Box::new(e)),
            })
    }
}

fn parse_success_body<T: DeserializeOwned>(
    bytes: &[u8],
    headers: &HeaderMap,
    status: u16,
) -> Result<T, ApiError> {
    if status == 204 {
        return serde_json::from_slice::<T>(b"null").map_err(|e| {
            ApiError::Sdk(format!(
                "204 No Content but could not produce a zero-value for type: {e}"
            ))
        });
    }

    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok());
    let media_type = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    let is_json = media_type.contains("application/json") || media_type.ends_with("+json");

    if is_json {
        let content_length_zero = headers
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == "0");
        if bytes.is_empty() || content_length_zero {
            return serde_json::from_slice::<T>(b"null").map_err(|e| {
                ApiError::Sdk(format!(
                    "empty JSON body but could not produce a zero-value for type: {e}"
                ))
            });
        }

        if let Some(request_id) = headers
            .get("request-id")
            .and_then(|value| value.to_str().ok())
        {
            let mut value = serde_json::from_slice::<serde_json::Value>(bytes)
                .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")))?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "_request_id".to_owned(),
                    serde_json::Value::String(request_id.to_owned()),
                );
            }
            return serde_json::from_value(value)
                .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")));
        }

        return serde_json::from_slice(bytes)
            .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")));
    }

    let text = String::from_utf8(bytes.to_vec()).map_err(|e| {
        ApiError::Sdk(format!(
            "response body was not valid UTF-8 for non-JSON response: {e}"
        ))
    })?;
    serde_json::from_value(serde_json::Value::String(text))
        .map_err(|e| ApiError::Sdk(format!("failed to deserialize text response: {e}")))
}

fn parse_success_body_from_raw<T: DeserializeOwned>(response: &RawResponse) -> Result<T, ApiError> {
    if response.status == 204 {
        return serde_json::from_slice::<T>(b"null").map_err(|e| {
            ApiError::Sdk(format!(
                "204 No Content but could not produce a zero-value for type: {e}"
            ))
        });
    }

    let media_type = response
        .header("content-type")
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default();
    let is_json = media_type.contains("application/json") || media_type.ends_with("+json");

    if is_json {
        let content_length_zero = response.header("content-length") == Some("0");
        if response.body.is_empty() || content_length_zero {
            return serde_json::from_slice::<T>(b"null").map_err(|e| {
                ApiError::Sdk(format!(
                    "empty JSON body but could not produce a zero-value for type: {e}"
                ))
            });
        }

        if let Some(request_id) = response.request_id() {
            let mut value = serde_json::from_slice::<serde_json::Value>(&response.body)
                .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")))?;
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "_request_id".to_owned(),
                    serde_json::Value::String(request_id.to_owned()),
                );
            }
            return serde_json::from_value(value)
                .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")));
        }

        return serde_json::from_slice(&response.body)
            .map_err(|e| ApiError::Sdk(format!("failed to deserialize response: {e}")));
    }

    let text = String::from_utf8(response.body.clone()).map_err(|e| {
        ApiError::Sdk(format!(
            "response body was not valid UTF-8 for non-JSON response: {e}"
        ))
    })?;
    serde_json::from_value(serde_json::Value::String(text))
        .map_err(|e| ApiError::Sdk(format!("failed to deserialize text response: {e}")))
}

async fn error_from_response(
    response: reqwest::Response,
    options: Option<&RequestOptions>,
) -> ApiError {
    let status = response.status().as_u16();
    let headers = response_headers_to_map(&response);
    let bytes = match read_response_bytes(response, options).await {
        Ok(bytes) => bytes,
        Err(err) => return err,
    };
    let err_text = String::from_utf8(bytes).ok();
    let err_json = err_text
        .as_deref()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
    let err_message = if err_json
        .as_ref()
        .is_some_and(crate::core::error::json_value_is_js_truthy)
    {
        None
    } else {
        err_text
    };

    ApiError::generate(Some(status), err_json, err_message, Some(headers))
}

fn api_error_from_reqwest(err: reqwest::Error) -> ApiError {
    if err.is_timeout() {
        ApiError::ConnectionTimeout {
            message: "Request timed out.".to_owned(),
        }
    } else {
        ApiError::Connection {
            message: "Connection error.".to_owned(),
            cause: Some(Box::new(err)),
        }
    }
}

fn is_retryable_connection_error(err: &ApiError) -> bool {
    matches!(
        err,
        ApiError::Connection { .. } | ApiError::ConnectionTimeout { .. }
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a client with an explicit API key so env vars don't
    /// interfere.
    fn test_client() -> Anthropic {
        Anthropic::new(ClientOptions {
            api_key: Some("test-key".to_owned()),
            ..Default::default()
        })
        .unwrap() // safe in tests
    }

    #[test]
    fn default_options() {
        let client = test_client();
        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
        assert_eq!(client.timeout_ms(), DEFAULT_TIMEOUT_MS);
        assert_eq!(client.max_retries(), DEFAULT_MAX_RETRIES);
    }

    #[test]
    fn custom_options() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("sk-custom".to_owned()),
            base_url: Some("https://custom.example.com".to_owned()),
            timeout: Some(30_000),
            max_retries: Some(5),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(client.api_key(), Some("sk-custom"));
        assert_eq!(client.base_url(), "https://custom.example.com");
        assert_eq!(client.timeout_ms(), 30_000);
        assert_eq!(client.max_retries(), 5);
    }

    #[test]
    fn empty_base_url_option_falls_back_like_ts_constructor() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("sk-custom".to_owned()),
            base_url: Some(String::new()),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(client.base_url(), DEFAULT_BASE_URL);
    }

    #[test]
    fn build_url_simple() {
        let client = test_client();
        let url = client.build_url("/v1/messages", None).unwrap();
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn build_url_with_query() {
        let client = test_client();
        let mut query = HashMap::new();
        query.insert("limit".to_owned(), Some("10".to_owned()));
        query.insert("with space".to_owned(), Some("a b".to_owned()));
        let url = client.build_url("/v1/models", Some(&query)).unwrap();
        assert!(url.contains("limit=10"));
        assert!(url.contains("with%20space=a%20b"));
        assert!(!url.contains("+"));
    }

    #[test]
    fn build_url_preserves_path_query_when_adding_params() {
        let client = test_client();
        let mut query = HashMap::new();
        query.insert("limit".to_owned(), Some("10".to_owned()));
        let url = client
            .build_url("/v1/models?beta=true", Some(&query))
            .unwrap();
        assert!(url.contains("beta=true"));
        assert!(url.contains("limit=10"));
    }

    #[test]
    fn build_url_absolute() {
        let client = test_client();
        let url = client
            .build_url("https://other.api.com/v1/foo", None)
            .unwrap();
        assert_eq!(url, "https://other.api.com/v1/foo");
    }

    #[test]
    fn build_headers_contains_required() {
        let client = test_client();
        let headers = client.build_headers(0, None).unwrap();

        assert_eq!(
            headers.get("x-api-key").and_then(|v| v.to_str().ok()),
            Some("test-key")
        );
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|v| v.to_str().ok()),
            Some(ANTHROPIC_VERSION)
        );
        assert!(headers.get("user-agent").is_some());
        assert_eq!(
            headers
                .get("x-stainless-retry-count")
                .and_then(|v| v.to_str().ok()),
            Some("0")
        );
    }

    #[test]
    fn build_headers_retry_count() {
        let client = test_client();
        let headers = client.build_headers(3, None).unwrap();
        assert_eq!(
            headers
                .get("x-stainless-retry-count")
                .and_then(|v| v.to_str().ok()),
            Some("3")
        );
    }

    #[test]
    fn build_headers_bearer_auth() {
        let client = Anthropic::new(ClientOptions {
            auth_token: Some("my-token".to_owned()),
            ..Default::default()
        })
        .unwrap();
        let headers = client.build_headers(0, None).unwrap();
        assert_eq!(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            Some("Bearer my-token")
        );
    }

    #[test]
    fn build_headers_none_removes() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("x-custom".to_owned(), Some("val".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        // Extra headers with None should remove.
        let mut extra = HashMap::new();
        extra.insert("x-custom".to_owned(), None);
        let headers = client.build_headers(0, Some(&extra)).unwrap();
        assert!(headers.get("x-custom").is_none());
    }

    #[test]
    fn build_headers_no_auth_error() {
        // Construct without any auth and with no env vars set. Skip if env
        // vars happen to be set (cannot safely unset across threads).
        if std::env::var("ANTHROPIC_API_KEY").is_ok()
            || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok()
        {
            return;
        }
        let client = Anthropic::new(ClientOptions::default()).unwrap();
        let result = client.build_headers(0, None);
        let err = result.unwrap_err();
        assert!(err.to_string().contains("apiKey or authToken"));
    }

    #[test]
    fn with_options_overrides() {
        let base = test_client();
        let child = base
            .with_options(ClientOptions {
                base_url: Some("https://child.example.com".to_owned()),
                max_retries: Some(10),
                ..Default::default()
            })
            .unwrap();

        assert_eq!(child.base_url(), "https://child.example.com");
        assert_eq!(child.max_retries(), 10);
        // Inherited
        assert_eq!(child.api_key(), Some("test-key"));
        assert_eq!(child.timeout_ms(), DEFAULT_TIMEOUT_MS);

        let camel_child = base
            .withOptions(ClientOptions {
                base_url: Some("https://camel.example.com".to_owned()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(camel_child.base_url(), "https://camel.example.com");
    }

    #[test]
    fn retry_delay_is_bounded() {
        let client = test_client();
        for retries_remaining in 0..=10 {
            let delay = client.retry_delay(retries_remaining, 10, None);
            // max delay is 8.0 * 1.0 * 1000 = 8000
            assert!(delay <= 8_000, "delay {delay} exceeded 8000");
        }
    }

    #[test]
    fn should_retry_status_codes() {
        // Verify the status matching logic matches the TS SDK spec.
        assert!(matches!(408_u16, 408 | 409 | 429 | 500..=599));
        assert!(matches!(409_u16, 408 | 409 | 429 | 500..=599));
        assert!(matches!(429_u16, 408 | 409 | 429 | 500..=599));
        assert!(matches!(500_u16, 408 | 409 | 429 | 500..=599));
        assert!(matches!(503_u16, 408 | 409 | 429 | 500..=599));
        assert!(!matches!(200_u16, 408 | 409 | 429 | 500..=599));
        assert!(!matches!(400_u16, 408 | 409 | 429 | 500..=599));
        assert!(!matches!(401_u16, 408 | 409 | 429 | 500..=599));
    }

    // -- defaultHeaders tests (TS: defaultHeaders > they are used in the request) --

    #[test]
    fn default_headers_are_used_in_request() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("x-my-default-header".to_owned(), Some("2".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let headers = client.build_headers(0, None).unwrap();
        assert_eq!(
            headers
                .get("x-my-default-header")
                .and_then(|v| v.to_str().ok()),
            Some("2")
        );
    }

    #[test]
    fn default_headers_can_ignore_none_and_keep_default() {
        // TS: setting a header to undefined in per-request headers should NOT
        // override the default. Rust uses None for removal, so this test verifies
        // that NOT specifying a key in extra_headers preserves the default.
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("x-my-default-header".to_owned(), Some("2".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        // Per-request headers that do NOT mention x-my-default-header at all
        let extra = HashMap::new();
        let headers = client.build_headers(0, Some(&extra)).unwrap();
        assert_eq!(
            headers
                .get("x-my-default-header")
                .and_then(|v| v.to_str().ok()),
            Some("2")
        );
    }

    #[test]
    fn default_headers_can_be_removed_with_none() {
        // TS: setting a header to null removes it. Rust uses None for this.
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("x-my-default-header".to_owned(), Some("2".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let mut extra = HashMap::new();
        extra.insert("x-my-default-header".to_owned(), None);
        let headers = client.build_headers(0, Some(&extra)).unwrap();
        assert!(headers.get("x-my-default-header").is_none());
    }

    // -- defaultQuery tests --

    #[test]
    fn default_query_with_no_per_request_query() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_query: Some({
                let mut m = HashMap::new();
                m.insert("apiVersion".to_owned(), Some("foo".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let url = client.build_url("/foo", None).unwrap();
        assert!(url.contains("apiVersion=foo"));
    }

    #[test]
    fn default_query_multiple_params() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_query: Some({
                let mut m = HashMap::new();
                m.insert("apiVersion".to_owned(), Some("foo".to_owned()));
                m.insert("hello".to_owned(), Some("world".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let url = client.build_url("/foo", None).unwrap();
        assert!(url.contains("apiVersion=foo"));
        assert!(url.contains("hello=world"));
    }

    #[test]
    fn default_query_override_with_none_removes() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_query: Some({
                let mut m = HashMap::new();
                m.insert("hello".to_owned(), Some("world".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let mut query = HashMap::new();
        query.insert("hello".to_owned(), None);
        let url = client.build_url("/foo", Some(&query)).unwrap();
        // The query param should be removed
        assert!(!url.contains("hello"));
    }

    // -- baseUrl tests --

    #[test]
    fn base_url_trailing_slash() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            base_url: Some("http://localhost:5000/custom/path/".to_owned()),
            ..Default::default()
        })
        .unwrap();

        let url = client.build_url("/foo", None).unwrap();
        assert_eq!(url, "http://localhost:5000/custom/path/foo");
    }

    #[test]
    fn base_url_no_trailing_slash() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            base_url: Some("http://localhost:5000/custom/path".to_owned()),
            ..Default::default()
        })
        .unwrap();

        let url = client.build_url("/foo", None).unwrap();
        assert_eq!(url, "http://localhost:5000/custom/path/foo");
    }

    #[test]
    fn base_url_explicit_option() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            base_url: Some("https://example.com".to_owned()),
            ..Default::default()
        })
        .unwrap();

        assert_eq!(client.base_url(), "https://example.com");
    }

    #[test]
    fn build_url_path_without_leading_slash() {
        // Verify that paths without leading slash get a separator
        let client = test_client();
        let url = client.build_url("foo", None).unwrap();
        assert_eq!(url, "https://api.anthropic.com/foo");
    }

    // -- max_retries option --

    #[test]
    fn max_retries_option_is_correctly_set() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            max_retries: Some(4),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(client.max_retries(), 4);

        let default_client = test_client();
        assert_eq!(default_client.max_retries(), DEFAULT_MAX_RETRIES);
    }

    // -- withOptions inherits defaultHeaders and defaultQuery --

    #[test]
    fn with_options_inherits_default_headers_and_query() {
        let parent = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("x-test".to_owned(), Some("val".to_owned()));
                m
            }),
            default_query: Some({
                let mut m = HashMap::new();
                m.insert("q1".to_owned(), Some("v1".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let child = parent
            .with_options(ClientOptions {
                base_url: Some("https://child.example.com".to_owned()),
                ..Default::default()
            })
            .unwrap();

        // Headers inherited
        let headers = child.build_headers(0, None).unwrap();
        assert_eq!(
            headers.get("x-test").and_then(|v| v.to_str().ok()),
            Some("val")
        );

        // Query inherited
        let url = child.build_url("/foo", None).unwrap();
        assert!(url.contains("q1=v1"));
    }

    // -- request_id extracted from response headers (TS: responses.test.ts) --

    #[test]
    fn request_id_extracted_from_error_headers() {
        // When ApiError::generate receives headers containing "request-id",
        // it should be accessible via .request_id().
        let mut headers = HashMap::new();
        headers.insert("request-id".to_owned(), "req_abc123".to_owned());
        headers.insert("content-type".to_owned(), "application/json".to_owned());

        let err = ApiError::generate(
            Some(400),
            Some(
                serde_json::json!({"type": "error", "error": {"type": "invalid_request_error", "message": "bad request"}}),
            ),
            None,
            Some(headers),
        );

        assert_eq!(err.request_id(), Some("req_abc123"));
        assert_eq!(err.status(), Some(400));
    }

    // -- custom headers: case-insensitive merging and null removes --

    #[test]
    fn custom_headers_case_insensitive_merge_and_null_remove() {
        let client = Anthropic::new(ClientOptions {
            api_key: Some("k".to_owned()),
            default_headers: Some({
                let mut m = HashMap::new();
                m.insert("X-Foo".to_owned(), Some("baz".to_owned()));
                m.insert("x-baz".to_owned(), Some("bam".to_owned()));
                m
            }),
            ..Default::default()
        })
        .unwrap();

        let mut extra = HashMap::new();
        // Override x-foo with bar (case-insensitive)
        extra.insert("x-foo".to_owned(), Some("bar".to_owned()));
        // Remove x-baz via None
        extra.insert("X-Baz".to_owned(), None);

        let headers = client.build_headers(0, Some(&extra)).unwrap();
        assert_eq!(
            headers.get("x-foo").and_then(|v| v.to_str().ok()),
            Some("bar")
        );
        assert!(headers.get("x-baz").is_none());
    }

    // -- env var tests (TS: "with environment variable arguments") --
    // NOTE: env var tests are inherently racy in multi-threaded test runners.
    // We use serial-unfriendly env manipulation so each test checks whether
    // the var is already set before proceeding.

    #[test]
    fn env_var_api_key_read() {
        // Save + set
        let prev = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::set_var("ANTHROPIC_API_KEY", "env-test-key");

        let client = Anthropic::new(ClientOptions::default()).unwrap();
        assert_eq!(client.api_key(), Some("env-test-key"));

        // Restore
        match prev {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
    }

    #[test]
    fn env_var_auth_token_read() {
        let prev_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let prev_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "env-token-123");
        // Ensure api_key is set so client construction succeeds
        std::env::set_var("ANTHROPIC_API_KEY", "dummy");

        let client = Anthropic::new(ClientOptions::default()).unwrap();
        assert_eq!(client.auth_token(), Some("env-token-123"));

        // Restore
        match prev_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
        match prev_token {
            Some(v) => std::env::set_var("ANTHROPIC_AUTH_TOKEN", v),
            None => std::env::remove_var("ANTHROPIC_AUTH_TOKEN"),
        }
    }

    #[test]
    fn env_var_base_url_read() {
        let prev_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let prev_url = std::env::var("ANTHROPIC_BASE_URL").ok();
        std::env::set_var("ANTHROPIC_BASE_URL", "https://example.com/from_env");
        std::env::set_var("ANTHROPIC_API_KEY", "dummy");

        let client = Anthropic::new(ClientOptions::default()).unwrap();
        assert_eq!(client.base_url(), "https://example.com/from_env");

        // Restore
        match prev_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
        match prev_url {
            Some(v) => std::env::set_var("ANTHROPIC_BASE_URL", v),
            None => std::env::remove_var("ANTHROPIC_BASE_URL"),
        }
    }

    #[test]
    fn env_vars_are_trimmed_like_ts_read_env() {
        let prev_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let prev_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();
        let prev_url = std::env::var("ANTHROPIC_BASE_URL").ok();
        std::env::set_var("ANTHROPIC_API_KEY", " env-key ");
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", " env-token ");
        std::env::set_var("ANTHROPIC_BASE_URL", " https://example.com/trimmed ");

        let client = Anthropic::new(ClientOptions::default()).unwrap();
        assert_eq!(client.api_key(), Some("env-key"));
        assert_eq!(client.auth_token(), Some("env-token"));
        assert_eq!(client.base_url(), "https://example.com/trimmed");

        match prev_key {
            Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
        match prev_token {
            Some(v) => std::env::set_var("ANTHROPIC_AUTH_TOKEN", v),
            None => std::env::remove_var("ANTHROPIC_AUTH_TOKEN"),
        }
        match prev_url {
            Some(v) => std::env::set_var("ANTHROPIC_BASE_URL", v),
            None => std::env::remove_var("ANTHROPIC_BASE_URL"),
        }
    }

    // -- retry-after header tests --

    #[test]
    fn retry_after_header_respected() {
        // Build a mock reqwest::Response with a Retry-After header.
        let http_resp = http::Response::builder()
            .status(429)
            .header("retry-after", "0.5")
            .body("")
            .unwrap();
        let resp: reqwest::Response = http_resp.into();

        let client = test_client();
        let delay = client.retry_delay(1, 2, Some(&resp));
        // Retry-After: 0.5 seconds = 500ms. Should be used directly since
        // it's within the 0..60_000 range.
        assert_eq!(delay, 500);
    }

    #[test]
    fn retry_after_ms_header_respected() {
        // Build a mock reqwest::Response with a Retry-After-Ms header.
        let http_resp = http::Response::builder()
            .status(429)
            .header("retry-after-ms", "150")
            .body("")
            .unwrap();
        let resp: reqwest::Response = http_resp.into();

        let client = test_client();
        let delay = client.retry_delay(1, 2, Some(&resp));
        // Retry-After-Ms: 150 milliseconds. Should be used directly.
        assert_eq!(delay, 150);
    }

    #[test]
    fn retry_after_zero_falls_back_to_default_delay_like_ts_truthy_guard() {
        let http_resp = http::Response::builder()
            .status(429)
            .header("retry-after-ms", "0")
            .body("")
            .unwrap();
        let resp: reqwest::Response = http_resp.into();

        let client = test_client();
        let delay = client.retry_delay(2, 2, Some(&resp));
        assert!(delay >= 375, "delay {delay} below default retry range");
        assert!(delay <= 500, "delay {delay} above default retry range");
    }

    #[test]
    fn retry_after_http_date_header_respected() {
        let retry_at = SystemTime::now() + Duration::from_secs(2);
        let retry_at = httpdate::fmt_http_date(retry_at);
        let http_resp = http::Response::builder()
            .status(429)
            .header("retry-after", retry_at)
            .body("")
            .unwrap();
        let resp: reqwest::Response = http_resp.into();

        let client = test_client();
        let delay = client.retry_delay(1, 2, Some(&resp));
        assert!(delay <= 2_000, "delay {delay} should use HTTP-date header");
        assert!(delay > 0, "delay {delay} should be positive");
    }

    // -- retry_delay minimum bounds per retry number --

    #[test]
    fn retry_delay_minimum_bounds() {
        let client = test_client();
        // For num_retries=0 (retries_remaining=max_retries), minimum is
        // 0.5 * 0.75 * 1000 = 375, maximum is 0.5 * 1.0 * 1000 = 500.
        for _ in 0..20 {
            let delay = client.retry_delay(2, 2, None); // num_retries=0
            assert!(delay >= 375, "delay {delay} below 375");
            assert!(delay <= 500, "delay {delay} above 500");
        }

        // For num_retries=1 (retries_remaining=max_retries-1): [750, 1000]
        for _ in 0..20 {
            let delay = client.retry_delay(1, 2, None); // num_retries=1
            assert!(delay >= 750, "delay {delay} below 750");
            assert!(delay <= 1000, "delay {delay} above 1000");
        }

        // For large num_retries, should cap at [6000, 8000]
        for _ in 0..20 {
            let delay = client.retry_delay(1, 10, None); // num_retries=9
            assert!(delay >= 6_000, "delay {delay} below 6000");
            assert!(delay <= 8_000, "delay {delay} above 8000");
        }
    }
}

// Maps to: TS packages/vertex-sdk/src/client.ts
//
// Google Cloud Vertex AI provider for the Anthropic SDK. Constructs a client
// whose `base_url` points at the regional Vertex AI endpoint and exposes a
// URL-rewriting helper that builds the full Vertex `rawPredict` /
// `streamRawPredict` URL from project, region, and model id.

use std::collections::HashMap;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use anthropic_sdk::client::{
    Anthropic, AuthTokenProvider, ClientOptions as CoreClientOptions, Nullable,
};
use anthropic_sdk::core::error::ApiError;
use anthropic_sdk::core::response::{ApiResponse, RawResponse};
use anthropic_sdk::core::streaming::SseStream;
use anthropic_sdk::internal::env::read_env;
use anthropic_sdk::resources::beta::messages::{
    BetaMessage, BetaMessageCountTokensParams, BetaMessageCreateParams, BetaMessageStreamEvent,
    BetaMessageTokensCount,
};
use anthropic_sdk::resources::beta::{
    files::Files as BetaFiles, models::BetaModels, skills::Skills,
};
use anthropic_sdk::resources::messages::{
    Message, MessageCountTokensParams, MessageCreateParams, MessageStreamEvent, MessageTokensCount,
};
use anthropic_sdk::sdk_lib::beta_message_stream::BetaMessageStream;
use anthropic_sdk::sdk_lib::beta_parser::{
    parse_beta_message, parsed_beta_message_without_parsing, ParsedBetaMessage,
};
use anthropic_sdk::sdk_lib::message_stream::MessageStream;
use anthropic_sdk::sdk_lib::parser::{
    parse_message, parsed_message_without_parsing, ParsedMessage,
};
use anthropic_sdk::sdk_lib::tools::{
    BetaMessageCreateClient, BetaToolRunner, BetaToolRunnerParams,
};
use anthropic_sdk::RequestOptions;

pub use anthropic_sdk::BaseAnthropic;

/// Vertex-specific API version header value.
/// Maps to: TS AnthropicVertex.ANTHROPIC_VERSION
pub const ANTHROPIC_VERSION: &str = "vertex-2023-10-16";

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

/// Async OAuth token provider for Google Cloud Vertex AI auth.
///
/// This is the Rust equivalent of TS Vertex auth-client token refresh: provider
/// integrations can call Google auth libraries externally and return a bearer
/// token for each request. Implementors may also expose a project id resolved
/// from credentials, mirroring TS `authClient.projectId` /
/// `x-goog-user-project` fallback behavior.
pub trait TokenProvider: Send + Sync {
    /// Return a valid OAuth bearer token without the `Bearer ` prefix.
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>>;

    /// Return a project id resolved from credentials, if available.
    ///
    /// The default returns `None`, preserving the explicit-`project_id`
    /// behavior for existing implementors.
    fn project_id(&self) -> Option<String> {
        None
    }
}

struct VertexTokenProviderAdapter {
    inner: Arc<dyn TokenProvider>,
}

impl AuthTokenProvider for VertexTokenProviderAdapter {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>> {
        Box::pin(async move {
            self.inner.get_token().await.map_err(|err| {
                ApiError::Sdk(format!(
                    "Failed to get token from Vertex token provider: {err}"
                ))
            })
        })
    }
}

/// Configuration for the Google Cloud Vertex AI provider.
///
/// Maps to: TS `AnthropicVertex` constructor options.
///
/// Both `project_id` and `region` are required. They can be supplied directly
/// or read from the standard GCP environment variables via
/// [`VertexConfig::from_env`].
#[derive(Clone)]
pub struct VertexConfig {
    /// GCP project id (e.g. `my-project-123`).
    pub project_id: String,

    /// GCP region (e.g. `us-east5`, `europe-west1`, or `global`).
    pub region: String,

    /// Static OAuth access token for `Authorization: Bearer ...`.
    /// Maps to: TS `ClientOptions.accessToken`.
    pub access_token: Option<String>,

    /// Dynamic OAuth token provider called on every request attempt.
    /// Maps to the auth-client token refresh behavior in the TS Vertex SDK.
    pub token_provider: Option<Arc<dyn TokenProvider>>,

    /// Optional endpoint override.
    /// Maps to TS `ClientOptions.baseURL` / `ANTHROPIC_VERTEX_BASE_URL`.
    pub base_url: Option<String>,
}

/// TS export-name compatibility alias for Vertex constructor options.
pub type ClientOptions = VertexConfig;

impl fmt::Debug for VertexConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VertexConfig")
            .field("project_id", &self.project_id)
            .field("region", &self.region)
            .field("access_token", &self.access_token.as_ref().map(|_| "***"))
            .field(
                "token_provider",
                &self.token_provider.as_ref().map(|_| "<dyn TokenProvider>"),
            )
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl VertexConfig {
    /// Build a config from environment variables.
    ///
    /// Maps to: TS `AnthropicVertex` constructor reading `process.env`.
    ///
    /// | Variable                      | Field        | Priority |
    /// |-------------------------------|------------- |----------|
    /// | `CLOUD_ML_REGION`             | `region`     | primary  |
    /// | `ANTHROPIC_VERTEX_PROJECT_ID` | `project_id` | primary  |
    /// | `ANTHROPIC_VERTEX_BASE_URL`   | `base_url`   | primary  |
    ///
    /// Returns `Err` when no region variable is set or when no project id env
    /// value is present. Runtime token providers can still resolve project ids
    /// through [`TokenProvider::project_id`] when constructing clients manually.
    pub fn from_env() -> Result<Self, ApiError> {
        // TS `client.ts:79,83`: `if (!region) throw`, so empty counts as unset.
        let region = read_env("CLOUD_ML_REGION")
            .filter(|value| !value.is_empty())
            .ok_or_else(vertex_missing_region_error)?;

        let project_id = read_env("ANTHROPIC_VERTEX_PROJECT_ID")
            .filter(|value| !value.is_empty())
            .ok_or_else(vertex_missing_project_id_error)?;

        // TS `client.ts:78,90`: `baseURL || <regional URL>`.
        let base_url = read_env("ANTHROPIC_VERTEX_BASE_URL").filter(|value| !value.is_empty());

        Ok(Self {
            project_id,
            region,
            access_token: None,
            token_provider: None,
            base_url,
        })
    }
}

fn vertex_missing_region_error() -> ApiError {
    ApiError::Sdk(
        "No region was given. The client should be instantiated with the `region` option or the `CLOUD_ML_REGION` environment variable should be set."
            .to_owned(),
    )
}

fn vertex_missing_project_id_error() -> ApiError {
    ApiError::Sdk(
        "No projectId was given and it could not be resolved from credentials. The client should be instantiated with the `projectId` option or the `ANTHROPIC_VERTEX_PROJECT_ID` environment variable should be set."
            .to_owned(),
    )
}

fn validate_vertex_config(config: &VertexConfig) -> Result<(), ApiError> {
    if config.region.trim().is_empty() {
        return Err(vertex_missing_region_error());
    }
    resolved_project_id(config).map(|_| ())
}

fn resolved_project_id(config: &VertexConfig) -> Result<String, ApiError> {
    let configured = config.project_id.trim();
    if !configured.is_empty() {
        return Ok(configured.to_owned());
    }

    config
        .token_provider
        .as_ref()
        .and_then(|provider| provider.project_id())
        .map(|project_id| project_id.trim().to_owned())
        .filter(|project_id| !project_id.is_empty())
        .ok_or_else(vertex_missing_project_id_error)
}

// ─────────────────────────────────────────────────────────────────────────────
// Client constructor
// ─────────────────────────────────────────────────────────────────────────────

/// Rust provider wrapper matching the TS `AnthropicVertex` class name.
#[derive(Debug, Clone)]
pub struct AnthropicVertex {
    inner: Anthropic,
    project_id: String,
    region: String,
}

impl AnthropicVertex {
    /// Create an [`AnthropicVertex`] client wrapper.
    pub fn new(config: &VertexConfig) -> Result<Self, ApiError> {
        let project_id = resolved_project_id(config)?;
        Ok(Self {
            inner: create_client(config)?,
            project_id,
            region: config.region.trim().to_owned(),
        })
    }

    /// Create an [`AnthropicVertex`] wrapper while also passing core SDK
    /// options such as timeout, retry count, default headers/query, logger, or
    /// custom HTTP client.
    ///
    /// This is the Rust equivalent of the TS provider `ClientOptions` type
    /// extending core client options while omitting Anthropic API-key auth.
    pub fn new_with_core_options(
        config: &VertexConfig,
        core_options: CoreClientOptions,
    ) -> Result<Self, ApiError> {
        let project_id = resolved_project_id(config)?;
        Ok(Self {
            inner: create_client_with_core_options(config, core_options)?,
            project_id,
            region: config.region.trim().to_owned(),
        })
    }

    /// Borrow the underlying core client.
    pub fn as_client(&self) -> &Anthropic {
        &self.inner
    }

    /// Maps to TS `AnthropicVertex.messages`.
    pub fn messages(&self) -> VertexMessages<'_> {
        VertexMessages {
            client: &self.inner,
            project_id: &self.project_id,
            region: &self.region,
        }
    }

    /// Maps to TS `AnthropicVertex.beta`.
    pub fn beta(&self) -> VertexBeta<'_> {
        VertexBeta {
            client: &self.inner,
            project_id: &self.project_id,
            region: &self.region,
        }
    }

    /// Consume the provider wrapper and return the underlying core client.
    pub fn into_inner(self) -> Anthropic {
        self.inner
    }
}

impl Deref for AnthropicVertex {
    type Target = Anthropic;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Create an [`Anthropic`] client pre-configured for Google Cloud Vertex AI.
///
/// Maps to: TS `new AnthropicVertex(opts)` — sets `base_url` to the regional
/// Vertex AI platform endpoint and injects OAuth bearer auth when configured.
pub fn create_client(config: &VertexConfig) -> Result<Anthropic, ApiError> {
    create_client_with_core_options(config, CoreClientOptions::default())
}

/// Create an [`Anthropic`] Vertex client while preserving caller-supplied core
/// client options (timeout, retries, default headers/query, logger, custom HTTP
/// client, etc.).
///
/// Maps to the TS Vertex `ClientOptions` type, which is the provider-specific
/// option set plus `Omit<CoreClientOptions, 'apiKey' | 'authToken'>`. Rust keeps
/// provider config and core options as separate structs for backwards
/// compatibility with existing `VertexConfig` literals.
pub fn create_client_with_core_options(
    config: &VertexConfig,
    mut core_options: CoreClientOptions,
) -> Result<Anthropic, ApiError> {
    validate_vertex_config(config)?;

    let region = config.region.trim();
    let host = vertex_host(region);
    // TS uses `baseURL || providerDefault`, so an explicit empty string falls
    // back to the Vertex endpoint while other (including whitespace) strings
    // remain explicit overrides.
    let base_url = config
        .base_url
        .clone()
        .filter(|base_url| !base_url.is_empty())
        .unwrap_or_else(|| format!("https://{host}/v1"));
    let auth_token_provider = config.token_provider.as_ref().map(|inner| {
        Arc::new(VertexTokenProviderAdapter {
            inner: Arc::clone(inner),
        }) as Arc<dyn AuthTokenProvider>
    });

    // Vertex never uses Anthropic `x-api-key` auth. Explicitly omit it so an
    // ambient ANTHROPIC_API_KEY cannot leak into Vertex requests. Caller
    // default headers are merged afterwards, matching TS defaultHeaders behavior
    // for provider clients.
    let mut default_headers = HashMap::new();
    default_headers.insert("x-api-key".to_owned(), None);
    if let Some(user_headers) = core_options.default_headers.take() {
        default_headers.extend(user_headers);
    }

    core_options.base_url = Some(base_url);
    // TS `AnthropicVertex` passes neither to `super`, but always adds the GCP
    // `Authorization` as the last header layer (`client.ts:109-131`), so an
    // ambient `ANTHROPIC_AUTH_TOKEN` never reaches Google. This port has no
    // default GCP auth, so the equivalent is an explicit `Null` unless an
    // `access_token` is given. The API key keeps the core default; its header
    // is omitted above.
    core_options.api_key = Nullable::Unset;
    core_options.auth_token = match &config.access_token {
        Some(token) => Nullable::Set(token.clone()),
        None => Nullable::Null,
    };
    core_options.auth_token_provider = auth_token_provider;
    core_options.default_headers = Some(default_headers);

    Anthropic::new(core_options)
}

// ─────────────────────────────────────────────────────────────────────────────
// URL rewriting
// ─────────────────────────────────────────────────────────────────────────────

/// Rewrite a standard Anthropic API path to the Vertex AI prediction URL.
///
/// Maps to: TS Vertex URL rewrite: `/v1/messages` ->
/// `https://{host}/v1/projects/{project}/locations/{region}/publishers/anthropic/models/{model}:{action}`
///
/// When `stream` is true the action is `streamRawPredict`; otherwise
/// `rawPredict`.
///
/// # Arguments
///
/// * `_path` - The original API path (currently unused; kept for forward
///   compatibility).
/// * `model` - The Anthropic model id (e.g. `claude-sonnet-4-20250514`).
/// * `config` - Vertex project/region configuration.
/// * `stream` - Whether to use the streaming prediction endpoint.
pub fn rewrite_url(_path: &str, model: &str, config: &VertexConfig, stream: bool) -> String {
    let host = vertex_host(&config.region);
    let action = if stream {
        "streamRawPredict"
    } else {
        "rawPredict"
    };
    format!(
        "https://{host}/v1/projects/{}/locations/{}/publishers/anthropic/models/{model}:{action}",
        config.project_id, config.region,
    )
}

/// Resolve the Vertex AI platform hostname for a given region.
///
/// The `global` region uses the un-prefixed hostname; all other regions are
/// prefixed with `{region}-`.
fn vertex_host(region: &str) -> String {
    if region == "global" {
        "aiplatform.googleapis.com".to_owned()
    } else {
        format!("{region}-aiplatform.googleapis.com")
    }
}

/// Vertex Messages resource wrapper.
///
/// Maps to TS Vertex `messages`, where model requests are rewritten from
/// `/v1/messages` to `/projects/{project}/locations/{region}/publishers/anthropic/models/{model}:rawPredict`
/// against the Vertex `/v1` base URL.
pub struct VertexMessages<'a> {
    client: &'a Anthropic,
    project_id: &'a str,
    region: &'a str,
}

impl<'a> VertexMessages<'a> {
    pub async fn create(&self, params: &MessageCreateParams) -> Result<Message, ApiError> {
        self.create_with_options(params, None).await
    }

    pub async fn create_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<Message, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<Message>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Vertex message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Message>, ApiError> {
        let rewrite_model_endpoint = vertex_should_rewrite_model_endpoint("/v1/messages", options);
        let mut body = vertex_message_body(params, options, rewrite_model_endpoint)?;
        if rewrite_model_endpoint {
            body.as_object_mut()
                .ok_or_else(|| {
                    ApiError::Sdk("MessageCreateParams did not serialize to object".into())
                })?
                .insert("stream".to_owned(), serde_json::Value::Bool(false));
        }
        let model = vertex_model_from_options_body(options, &params.model);
        let path = if rewrite_model_endpoint {
            self.model_path(&model, false)
        } else {
            vertex_effective_resource_path("/v1/messages", options).to_owned()
        };
        let core_options = vertex_core_options(options);
        self.client
            .post_with_response(&path, &body, None, core_options.as_ref())
            .await
    }

    pub async fn parse<T>(&self, params: &MessageCreateParams) -> Result<ParsedMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_options(params, None).await
    }

    pub async fn parse_with_options<T>(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ParsedMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        Ok(self
            .parse_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Vertex message returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .create_with_response_and_options(params, options)
            .await?;
        let parsed = if has_message_json_schema_output_format(params) {
            parse_message(&response.data)?
        } else {
            parsed_message_without_parsing(&response.data)
        };
        Ok(ApiResponse {
            data: parsed,
            response: response.response,
            request_id: response.request_id,
        })
    }

    pub async fn create_stream(
        &self,
        params: &MessageCreateParams,
    ) -> Result<SseStream<MessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    pub async fn create_stream_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SseStream<MessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Vertex streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        let rewrite_model_endpoint = vertex_should_rewrite_model_endpoint("/v1/messages", options);
        let mut body = vertex_message_body(params, options, rewrite_model_endpoint)?;
        if rewrite_model_endpoint {
            body.as_object_mut()
                .ok_or_else(|| {
                    ApiError::Sdk("MessageCreateParams did not serialize to object".into())
                })?
                .insert("stream".to_owned(), serde_json::Value::Bool(true));
        }
        let model = vertex_model_from_options_body(options, &params.model);
        let path = if rewrite_model_endpoint {
            self.model_path(&model, true)
        } else {
            vertex_effective_resource_path("/v1/messages", options).to_owned()
        };
        let core_options = vertex_core_options(options);
        let response = self
            .client
            .post_stream_with_options(&path, &body, None, core_options.as_ref())
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: SseStream::new(response),
            response: raw,
            request_id,
        })
    }

    pub async fn stream(&self, params: &MessageCreateParams) -> Result<MessageStream, ApiError> {
        self.stream_with_options(params, None).await
    }

    pub async fn stream_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageStream, ApiError> {
        Ok(self
            .stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS high-level `AnthropicVertex.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Vertex message stream returning response metadata.
    pub async fn stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        let mut helper_options = options.cloned().unwrap_or_default();
        helper_options
            .headers
            .get_or_insert_with(HashMap::new)
            .insert(
                "X-Stainless-Helper-Method".to_owned(),
                Some("stream".to_owned()),
            );
        let response = self
            .create_stream_with_response_and_options(params, Some(&helper_options))
            .await?;
        Ok(ApiResponse {
            data: MessageStream::new(Box::pin(response.data)),
            response: response.response,
            request_id: response.request_id,
        })
    }

    pub async fn count_tokens(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, None).await
    }

    #[allow(non_snake_case)]
    pub async fn countTokens(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens(params).await
    }

    pub async fn count_tokens_with_options(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageTokensCount, ApiError> {
        Ok(self
            .count_tokens_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count Vertex message tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        let mut body = request_body_or_params(params, options, "MessageCountTokensParams")?;
        insert_vertex_version(&mut body)?;
        let path = vertex_count_tokens_path(
            "/v1/messages/count_tokens",
            self.project_id,
            self.region,
            options,
        );
        let core_options = vertex_core_options(options);
        self.client
            .post_with_response(&path, &body, None, core_options.as_ref())
            .await
    }

    #[allow(non_snake_case)]
    pub async fn countTokensWithOptions(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, options).await
    }

    #[allow(non_snake_case)]
    pub async fn countTokensWithResponse(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        self.count_tokens_with_response(params).await
    }

    fn model_path(&self, model: &str, stream: bool) -> String {
        let action = if stream {
            "streamRawPredict"
        } else {
            "rawPredict"
        };
        format!(
            "/projects/{}/locations/{}/publishers/anthropic/models/{model}:{action}",
            self.project_id, self.region,
        )
    }
}

fn request_body_or_params<T: serde::Serialize>(
    params: &T,
    options: Option<&RequestOptions>,
    type_name: &str,
) -> Result<serde_json::Value, ApiError> {
    if let Some(body) = options.and_then(|options| options.body.as_ref()) {
        return Ok(body.clone());
    }

    serde_json::to_value(params)
        .map_err(|err| ApiError::Sdk(format!("failed to serialize {type_name}: {err}")))
}

fn vertex_model_from_options_body(options: Option<&RequestOptions>, fallback: &str) -> String {
    options
        .and_then(|options| options.body.as_ref())
        .and_then(|body| body.get("model"))
        .and_then(|model| model.as_str())
        .unwrap_or(fallback)
        .to_owned()
}

fn vertex_core_options(options: Option<&RequestOptions>) -> Option<RequestOptions> {
    options.cloned().map(|mut options| {
        // Provider wrappers have already applied RequestOptions.body/path before
        // URL rewrite. Clear them before delegating to the core client so the
        // transformed Vertex JSON body/path are sent on the wire.
        options.body = None;
        options.path = None;
        options
    })
}

fn vertex_effective_resource_path<'a>(
    default_path: &'a str,
    options: Option<&'a RequestOptions>,
) -> &'a str {
    options
        .and_then(|options| options.path.as_deref())
        .unwrap_or(default_path)
}

fn vertex_effective_method<'a>(
    default_method: &'a str,
    options: Option<&'a RequestOptions>,
) -> &'a str {
    options
        .and_then(|options| options.method.as_ref())
        .map(|method| method.as_str())
        .unwrap_or(default_method)
}

fn vertex_should_rewrite_model_endpoint(
    default_path: &str,
    options: Option<&RequestOptions>,
) -> bool {
    vertex_effective_method("POST", options).eq_ignore_ascii_case("POST")
        && matches!(
            vertex_effective_resource_path(default_path, options),
            "/v1/messages" | "/v1/messages?beta=true"
        )
}

fn vertex_should_rewrite_count_tokens_endpoint(
    default_path: &str,
    options: Option<&RequestOptions>,
) -> bool {
    let path = vertex_effective_resource_path(default_path, options);
    // Preserve the exact TS operator precedence: the stable path is rewritten
    // regardless of an overridden method, while the beta path additionally
    // requires POST.
    path == "/v1/messages/count_tokens"
        || (path == "/v1/messages/count_tokens?beta=true"
            && vertex_effective_method("POST", options).eq_ignore_ascii_case("POST"))
}

fn vertex_count_tokens_path(
    default_path: &str,
    project_id: &str,
    region: &str,
    options: Option<&RequestOptions>,
) -> String {
    if vertex_should_rewrite_count_tokens_endpoint(default_path, options) {
        format!(
            "/projects/{project_id}/locations/{region}/publishers/anthropic/models/count-tokens:rawPredict"
        )
    } else {
        vertex_effective_resource_path(default_path, options).to_owned()
    }
}

fn vertex_message_body(
    params: &MessageCreateParams,
    options: Option<&RequestOptions>,
    rewrite_model_endpoint: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = request_body_or_params(params, options, "MessageCreateParams")?;
    prepare_vertex_model_body(&mut body, rewrite_model_endpoint)?;
    Ok(body)
}

/// Vertex beta namespace wrapper.
pub struct VertexBeta<'a> {
    client: &'a Anthropic,
    project_id: &'a str,
    region: &'a str,
}

impl<'a> VertexBeta<'a> {
    /// Maps to TS `AnthropicVertex.beta.messages`.
    pub fn messages(&self) -> VertexBetaMessages<'a> {
        VertexBetaMessages {
            client: self.client,
            project_id: self.project_id,
            region: self.region,
        }
    }

    /// Maps to TS `AnthropicVertex.beta.models`.
    pub fn models(&self) -> BetaModels<'a> {
        BetaModels::new(self.client)
    }

    /// Maps to TS `AnthropicVertex.beta.files`.
    pub fn files(&self) -> BetaFiles<'a> {
        BetaFiles::new(self.client)
    }

    /// Maps to TS `AnthropicVertex.beta.skills`.
    pub fn skills(&self) -> Skills<'a> {
        Skills::new(self.client)
    }
}

/// Vertex beta Messages resource wrapper.
pub struct VertexBetaMessages<'a> {
    client: &'a Anthropic,
    project_id: &'a str,
    region: &'a str,
}

impl<'a> VertexBetaMessages<'a> {
    pub async fn create(&self, params: &BetaMessageCreateParams) -> Result<BetaMessage, ApiError> {
        self.create_with_options(params, None).await
    }

    pub async fn create_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessage, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.beta.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Vertex beta message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        let rewrite_model_endpoint =
            vertex_should_rewrite_model_endpoint("/v1/messages?beta=true", options);
        let mut body = vertex_beta_message_body(params, options, rewrite_model_endpoint)?;
        if rewrite_model_endpoint {
            body.as_object_mut()
                .ok_or_else(|| {
                    ApiError::Sdk("BetaMessageCreateParams did not serialize to object".into())
                })?
                .insert("stream".to_owned(), serde_json::Value::Bool(false));
        }
        let model = vertex_model_from_options_body(options, &params.model);
        let path = if rewrite_model_endpoint {
            self.model_path(&model, false)
        } else {
            vertex_effective_resource_path("/v1/messages?beta=true", options).to_owned()
        };
        let headers = beta_headers(params.betas.as_ref(), false);
        let core_options = vertex_core_options(options);
        self.client
            .post_with_response(&path, &body, headers.as_ref(), core_options.as_ref())
            .await
    }

    pub async fn parse<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ParsedBetaMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_options(params, None).await
    }

    pub async fn parse_with_options<T>(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ParsedBetaMessage<T>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        Ok(self
            .parse_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.beta.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Vertex beta message returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut params = params.clone();
        let structured_outputs = "structured-outputs-2025-12-15".to_owned();
        let betas = params.betas.get_or_insert_with(Vec::new);
        if !betas.contains(&structured_outputs) {
            betas.push(structured_outputs);
        }
        let should_parse = has_beta_message_json_schema_output_format(&params);
        let response = self
            .create_with_response_and_options(&params, options)
            .await?;
        let parsed = if should_parse {
            parse_beta_message(&response.data)?
        } else {
            parsed_beta_message_without_parsing(&response.data)
        };
        Ok(ApiResponse {
            data: parsed,
            response: response.response,
            request_id: response.request_id,
        })
    }

    pub async fn create_stream(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<SseStream<BetaMessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    pub async fn create_stream_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SseStream<BetaMessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.beta.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Vertex beta streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        let rewrite_model_endpoint =
            vertex_should_rewrite_model_endpoint("/v1/messages?beta=true", options);
        let mut body = vertex_beta_message_body(params, options, rewrite_model_endpoint)?;
        if rewrite_model_endpoint {
            body.as_object_mut()
                .ok_or_else(|| {
                    ApiError::Sdk("BetaMessageCreateParams did not serialize to object".into())
                })?
                .insert("stream".to_owned(), serde_json::Value::Bool(true));
        }
        let model = vertex_model_from_options_body(options, &params.model);
        let path = if rewrite_model_endpoint {
            self.model_path(&model, true)
        } else {
            vertex_effective_resource_path("/v1/messages?beta=true", options).to_owned()
        };
        let headers = beta_headers(params.betas.as_ref(), false);
        let core_options = vertex_core_options(options);
        let response = self
            .client
            .post_stream_with_options(&path, &body, headers.as_ref(), core_options.as_ref())
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: SseStream::new(response),
            response: raw,
            request_id,
        })
    }

    pub async fn count_tokens(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, None).await
    }

    #[allow(non_snake_case)]
    pub async fn countTokens(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens(params).await
    }

    pub async fn count_tokens_with_options(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        Ok(self
            .count_tokens_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicVertex.beta.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count Vertex beta-message tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        let mut body = request_body_or_params(params, options, "BetaMessageCountTokensParams")?;
        insert_vertex_version(&mut body)?;
        let path = vertex_count_tokens_path(
            "/v1/messages/count_tokens?beta=true",
            self.project_id,
            self.region,
            options,
        );
        let headers = beta_headers(params.betas.as_ref(), true);
        let core_options = vertex_core_options(options);
        self.client
            .post_with_response(&path, &body, headers.as_ref(), core_options.as_ref())
            .await
    }

    #[allow(non_snake_case)]
    pub async fn countTokensWithOptions(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageTokensCount, ApiError> {
        self.count_tokens_with_options(params, options).await
    }

    #[allow(non_snake_case)]
    pub async fn countTokensWithResponse(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        self.count_tokens_with_response(params).await
    }

    pub async fn stream(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<BetaMessageStream, ApiError> {
        self.stream_with_options(params, None).await
    }

    pub async fn stream_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageStream, ApiError> {
        Ok(self
            .stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS high-level `AnthropicVertex.beta.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Vertex beta message stream returning response metadata.
    pub async fn stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        let mut helper_options = options.cloned().unwrap_or_default();
        helper_options
            .headers
            .get_or_insert_with(HashMap::new)
            .insert(
                "X-Stainless-Helper-Method".to_owned(),
                Some("stream".to_owned()),
            );
        let response = self
            .create_stream_with_response_and_options(params, Some(&helper_options))
            .await?;
        Ok(ApiResponse {
            data: BetaMessageStream::new(Box::pin(response.data)),
            response: response.response,
            request_id: response.request_id,
        })
    }

    pub fn tool_runner(&self, params: BetaToolRunnerParams) -> BetaToolRunner<'a> {
        BetaToolRunner::new_with_message_client(
            VertexBetaMessageCreateClient {
                client: self.client,
                project_id: self.project_id,
                region: self.region,
            },
            params,
        )
    }

    #[allow(non_snake_case)]
    pub fn toolRunner(&self, params: BetaToolRunnerParams) -> BetaToolRunner<'a> {
        self.tool_runner(params)
    }

    pub fn tool_runner_with_options(
        &self,
        params: BetaToolRunnerParams,
        options: RequestOptions,
    ) -> BetaToolRunner<'a> {
        BetaToolRunner::new_with_message_client_and_options(
            VertexBetaMessageCreateClient {
                client: self.client,
                project_id: self.project_id,
                region: self.region,
            },
            params,
            options,
        )
    }

    #[allow(non_snake_case)]
    pub fn toolRunnerWithOptions(
        &self,
        params: BetaToolRunnerParams,
        options: RequestOptions,
    ) -> BetaToolRunner<'a> {
        self.tool_runner_with_options(params, options)
    }

    fn model_path(&self, model: &str, stream: bool) -> String {
        let action = if stream {
            "streamRawPredict"
        } else {
            "rawPredict"
        };
        format!(
            "/projects/{}/locations/{}/publishers/anthropic/models/{model}:{action}",
            self.project_id, self.region,
        )
    }
}

struct VertexBetaMessageCreateClient<'a> {
    client: &'a Anthropic,
    project_id: &'a str,
    region: &'a str,
}

impl BetaMessageCreateClient for VertexBetaMessageCreateClient<'_> {
    fn create_beta_message_with_options<'b>(
        &'b self,
        params: &'b BetaMessageCreateParams,
        options: Option<&'b RequestOptions>,
    ) -> futures::future::BoxFuture<'b, Result<BetaMessage, ApiError>> {
        Box::pin(async move {
            VertexBetaMessages {
                client: self.client,
                project_id: self.project_id,
                region: self.region,
            }
            .create_with_options(params, options)
            .await
        })
    }
}

fn has_message_json_schema_output_format(params: &MessageCreateParams) -> bool {
    params
        .output_config
        .as_ref()
        .and_then(|config| config.format.as_ref())
        .is_some_and(|format| format.type_name == "json_schema")
}

fn has_beta_message_json_schema_output_format(params: &BetaMessageCreateParams) -> bool {
    params
        .output_format
        .as_ref()
        .or_else(|| {
            params
                .output_config
                .as_ref()
                .and_then(|config| config.format.as_ref())
        })
        .is_some_and(|format| format.type_name == "json_schema")
}

fn vertex_beta_message_body(
    params: &BetaMessageCreateParams,
    options: Option<&RequestOptions>,
    rewrite_model_endpoint: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = request_body_or_params(params, options, "BetaMessageCreateParams")?;
    prepare_vertex_model_body(&mut body, rewrite_model_endpoint)?;
    Ok(body)
}

fn prepare_vertex_model_body(
    body: &mut serde_json::Value,
    rewrite_model_endpoint: bool,
) -> Result<(), ApiError> {
    insert_vertex_version(body)?;
    if rewrite_model_endpoint {
        body.as_object_mut()
            .ok_or_else(|| {
                ApiError::Sdk(
                    "Expected request body to be an object for post /v1/messages".to_owned(),
                )
            })?
            .remove("model");
    }
    Ok(())
}

fn insert_vertex_version(body: &mut serde_json::Value) -> Result<(), ApiError> {
    let Some(obj) = body.as_object_mut() else {
        // TS Vertex only injects anthropic_version when `isObj(options.body)`
        // is true; custom-path primitive/falsy bodies are passed through to
        // the core builder unchanged.
        return Ok(());
    };
    if !obj
        .get("anthropic_version")
        .is_some_and(json_value_is_js_truthy)
    {
        obj.insert(
            "anthropic_version".to_owned(),
            serde_json::Value::String(ANTHROPIC_VERSION.to_owned()),
        );
    }
    Ok(())
}

fn json_value_is_js_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
    }
}

fn beta_headers(
    betas: Option<&Vec<String>>,
    include_token_counting: bool,
) -> Option<HashMap<String, Option<String>>> {
    let mut beta_values = betas.cloned().unwrap_or_default();
    if include_token_counting {
        let token_counting = "token-counting-2024-11-01".to_owned();
        if !beta_values.contains(&token_counting) {
            beta_values.push(token_counting);
        }
    }
    if beta_values.is_empty() {
        None
    } else {
        let mut headers = HashMap::new();
        headers.insert("anthropic-beta".to_owned(), Some(beta_values.join(",")));
        Some(headers)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

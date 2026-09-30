// Maps to: TS packages/foundry-sdk/src/client.ts
//
// Azure AI Foundry provider for the Anthropic SDK. Constructs a client whose
// `base_url` points at the Azure AI Services Anthropic endpoint and supports
// both static API-key and dynamic token-provider authentication.

use std::ops::Deref;
use std::sync::Arc;

use anthropic_sdk::client::{
    Anthropic, AuthTokenProvider, ClientOptions as CoreClientOptions, Nullable,
};
use anthropic_sdk::core::error::ApiError;
use anthropic_sdk::core::response::ApiResponse;
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
use anthropic_sdk::sdk_lib::beta_parser::ParsedBetaMessage;
use anthropic_sdk::sdk_lib::message_stream::MessageStream;
use anthropic_sdk::sdk_lib::parser::ParsedMessage;
use anthropic_sdk::sdk_lib::tools::{
    BetaMessageCreateClient, BetaToolRunner, BetaToolRunnerParams,
};
use anthropic_sdk::RequestOptions;

pub use anthropic_sdk::BaseAnthropic;

// ─────────────────────────────────────────────────────────────────────────────
// Token provider trait
// ─────────────────────────────────────────────────────────────────────────────

/// An async token provider for Azure AD / Entra ID credential flows.
///
/// Maps to: TS `tokenProvider?: () => Promise<string>` in
/// `AnthropicFoundryOptions`.
///
/// Implement this trait to supply bearer tokens that rotate over time (e.g.
/// via `DefaultAzureCredential`). The provider is called once per request, so
/// implementors should cache tokens internally and only refresh when needed.
pub trait TokenProvider: Send + Sync {
    /// Return a valid bearer token.
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>>;
}

struct FoundryTokenProviderAdapter {
    inner: Box<dyn TokenProvider>,
}

impl AuthTokenProvider for FoundryTokenProviderAdapter {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, ApiError>> {
        Box::pin(async move {
            // TS rethrows provider-produced AnthropicError values unchanged.
            // Rust token providers already return ApiError, so preserve it.
            let token = self.inner.get_token().await?;
            if token.is_empty() {
                return Err(ApiError::Sdk(
                    "Expected azureADTokenProvider function argument to return a string but it returned "
                        .to_owned(),
                ));
            }
            Ok(token)
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the Azure AI Foundry provider.
///
/// Maps to: TS `AnthropicFoundry` constructor options.
///
/// Exactly one of `api_key` or `token_provider` must be set. Supplying both
/// (or neither) is an error. Exactly one endpoint selector must also be set:
/// either `resource` or `base_url`. Because Rust struct fields are not
/// optional by default, use an empty `resource` string when constructing a
/// base-URL-only config.
pub struct FoundryConfig {
    /// Azure AI Services resource name (the `{name}` part of
    /// `{name}.services.ai.azure.com`). Leave empty when `base_url` is set.
    pub resource: String,

    /// Static API key for key-based authentication.
    pub api_key: Option<String>,

    /// Dynamic token provider for Azure AD / Entra ID credential flows.
    pub token_provider: Option<Box<dyn TokenProvider>>,

    /// Optional endpoint override.
    /// Maps to TS `ClientOptions.baseURL` / `ANTHROPIC_FOUNDRY_BASE_URL`.
    pub base_url: Option<String>,
}

/// TS export-name compatibility alias for Foundry constructor options.
pub type FoundryClientOptions = FoundryConfig;

impl std::fmt::Debug for FoundryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FoundryConfig")
            .field("resource", &self.resource)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field(
                "token_provider",
                &if self.token_provider.is_some() {
                    "Some(<dyn TokenProvider>)"
                } else {
                    "None"
                },
            )
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl FoundryConfig {
    /// Build a config from environment variables.
    ///
    /// Maps to: TS `AnthropicFoundry` constructor reading `process.env`.
    ///
    /// | Variable                        | Field      | Priority |
    /// |---------------------------------|------------|----------|
    /// | `ANTHROPIC_FOUNDRY_RESOURCE`    | `resource` | primary  |
    /// | `ANTHROPIC_FOUNDRY_API_KEY`     | `api_key`  | primary  |
    /// | `ANTHROPIC_FOUNDRY_BASE_URL`    | `base_url` | primary  |
    ///
    /// `token_provider` cannot be set from the environment; callers that need
    /// dynamic tokens should construct the config directly.
    ///
    /// Returns `Err` when neither `resource` nor `base_url` is present.
    pub fn from_env() -> Result<Self, ApiError> {
        // TS `client.ts:58-60` then tests each for truthiness (`!apiKey`,
        // `!baseURL`, `resource ?`), so empty counts as unset.
        let resource = read_env("ANTHROPIC_FOUNDRY_RESOURCE")
            .filter(|value| !value.is_empty())
            .unwrap_or_default();

        let api_key = read_env("ANTHROPIC_FOUNDRY_API_KEY").filter(|value| !value.is_empty());
        let base_url = read_env("ANTHROPIC_FOUNDRY_BASE_URL").filter(|value| !value.is_empty());

        let config = Self {
            resource,
            api_key,
            token_provider: None,
            base_url,
        };
        validate_endpoint(&config)?;
        Ok(config)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation
// ─────────────────────────────────────────────────────────────────────────────

/// Validate that exactly one authentication method is configured.
///
/// Maps to: TS `AnthropicFoundry` constructor validation — "`apiKey` and
/// `azureADTokenProvider` arguments are mutually exclusive".
fn validate_auth(config: &FoundryConfig) -> Result<(), ApiError> {
    // Match the TS constructor's JavaScript truthiness checks: an explicit
    // empty API key behaves as missing, while whitespace remains truthy.
    let has_api_key = config
        .api_key
        .as_ref()
        .is_some_and(|api_key| !api_key.is_empty());
    let has_token_provider = config.token_provider.is_some();

    if has_api_key && has_token_provider {
        return Err(ApiError::Sdk(
            "The `apiKey` and `azureADTokenProvider` arguments are mutually exclusive; only one can be passed at a time."
                .to_owned(),
        ));
    }
    if !has_api_key && !has_token_provider {
        return Err(ApiError::Sdk(
            "Missing credentials. Please pass one of `apiKey` and `azureTokenProvider`, or set the `ANTHROPIC_FOUNDRY_API_KEY` environment variable."
                .to_owned(),
        ));
    }
    Ok(())
}

/// Validate endpoint selection, matching TS Foundry constructor behavior:
/// callers must provide exactly one of `baseURL` and `resource`.
fn validate_endpoint(config: &FoundryConfig) -> Result<(), ApiError> {
    // Explicit constructor values follow JS truthiness. Values loaded by
    // from_env() have already gone through readEnv-style trimming.
    let has_resource = !config.resource.is_empty();
    let has_base_url = config
        .base_url
        .as_deref()
        .is_some_and(|value| !value.is_empty());

    if has_base_url && has_resource {
        return Err(ApiError::Sdk(
            "baseURL and resource are mutually exclusive".to_owned(),
        ));
    }

    if !has_base_url && !has_resource {
        return Err(ApiError::Sdk(
            "Must provide one of the `baseURL` or `resource` arguments, or the `ANTHROPIC_FOUNDRY_RESOURCE` environment variable"
                .to_owned(),
        ));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Base URL
// ─────────────────────────────────────────────────────────────────────────────

/// Build the Foundry base URL from a resource name.
///
/// Maps to: TS `AnthropicFoundry` base URL construction.
///
/// ```
/// assert_eq!(
///     anthropic_sdk_foundry::base_url("my-resource"),
///     "https://my-resource.services.ai.azure.com/anthropic/"
/// );
/// ```
pub fn base_url(resource: &str) -> String {
    format!("https://{resource}.services.ai.azure.com/anthropic/")
}

// ─────────────────────────────────────────────────────────────────────────────
// Client constructor
// ─────────────────────────────────────────────────────────────────────────────

/// Rust provider wrapper matching the TS `AnthropicFoundry` class name.
#[derive(Debug, Clone)]
pub struct AnthropicFoundry {
    inner: Anthropic,
}

impl AnthropicFoundry {
    /// Create an [`AnthropicFoundry`] client wrapper.
    pub fn new(config: FoundryConfig) -> Result<Self, ApiError> {
        Ok(Self {
            inner: create_client(config)?,
        })
    }

    /// Create an [`AnthropicFoundry`] wrapper while also passing core SDK
    /// options such as timeout, retry count, default headers/query, logger, or
    /// custom HTTP client.
    ///
    /// This is the Rust equivalent of the TS provider options extending core
    /// client options while keeping Foundry auth configured by [`FoundryConfig`].
    pub fn new_with_core_options(
        config: FoundryConfig,
        core_options: CoreClientOptions,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            inner: create_client_with_core_options(config, core_options)?,
        })
    }

    /// Borrow the underlying core client.
    pub fn as_client(&self) -> &Anthropic {
        &self.inner
    }

    /// Maps to TS `AnthropicFoundry.messages`.
    pub fn messages(&self) -> FoundryMessages<'_> {
        FoundryMessages {
            client: &self.inner,
        }
    }

    /// Maps to TS `AnthropicFoundry.beta`.
    pub fn beta(&self) -> FoundryBeta<'_> {
        FoundryBeta {
            client: &self.inner,
        }
    }

    /// Consume the provider wrapper and return the underlying core client.
    pub fn into_inner(self) -> Anthropic {
        self.inner
    }
}

impl Deref for AnthropicFoundry {
    type Target = Anthropic;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Create an [`Anthropic`] client pre-configured for Azure AI Foundry.
///
/// Maps to: TS `new AnthropicFoundry(opts)` — sets `base_url` to the
/// resource-scoped Anthropic endpoint and, when an `api_key` is present,
/// passes it as the client API key.
///
/// Returns `Err` if both `api_key` and `token_provider` are set, or if
/// neither is set.
pub fn create_client(config: FoundryConfig) -> Result<Anthropic, ApiError> {
    create_client_with_core_options(config, CoreClientOptions::default())
}

/// Create an [`Anthropic`] Foundry client while preserving caller-supplied core
/// client options (timeout, retries, default headers/query, logger, custom HTTP
/// client, etc.).
///
/// Maps to the TS Foundry provider options extending core client options while
/// keeping API-key / token-provider auth controlled by [`FoundryConfig`]. Rust
/// keeps provider config and core options as separate structs for backwards
/// compatibility with existing `FoundryConfig` literals.
pub fn create_client_with_core_options(
    config: FoundryConfig,
    mut core_options: CoreClientOptions,
) -> Result<Anthropic, ApiError> {
    validate_auth(&config)?;
    validate_endpoint(&config)?;

    let url = config
        .base_url
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| base_url(&config.resource));
    let auth_token_provider = config
        .token_provider
        .map(|inner| Arc::new(FoundryTokenProviderAdapter { inner }) as Arc<dyn AuthTokenProvider>);

    core_options.base_url = Some(url);
    // TS `AnthropicFoundry` passes `apiKey: azureADTokenProvider ?? apiKey`
    // and overrides `authHeaders` (`client.ts:96,103-131`): with a token
    // provider only its Bearer token is sent, otherwise only the Foundry key,
    // and never `ANTHROPIC_API_KEY` or `ANTHROPIC_AUTH_TOKEN`. So the token is
    // always `Null`, and so is the key in token mode, where `validate_auth`
    // lets an empty key through beside the provider.
    core_options.api_key = if auth_token_provider.is_some() {
        Nullable::Null
    } else {
        Nullable::from_resolved(config.api_key)
    };
    core_options.auth_token = Nullable::Null;
    core_options.auth_token_provider = auth_token_provider;
    // TS `validateHeaders() {}` (`client.ts:133-135`). `validate_auth` above
    // already requires the key or the token provider, as the TS constructor
    // does.
    core_options.skip_auth_validation = true;

    Anthropic::new(core_options)
}

/// Foundry Messages resource wrapper.
///
/// Maps to TS Foundry `messages`, which omits the unsupported Batch API while
/// retaining create/stream/count-tokens behavior from the core resource.
pub struct FoundryMessages<'a> {
    client: &'a Anthropic,
}

impl<'a> FoundryMessages<'a> {
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

    /// Rust equivalent of TS `AnthropicFoundry.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<Message>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Foundry message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Message>, ApiError> {
        let messages = self.client.messages();
        messages
            .create_with_response_and_options(params, options)
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

    /// Rust equivalent of TS `AnthropicFoundry.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Foundry message returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let messages = self.client.messages();
        messages
            .parse_with_response_and_options(params, options)
            .await
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

    /// Rust equivalent of TS `AnthropicFoundry.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Foundry streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<MessageStreamEvent>>, ApiError> {
        let messages = self.client.messages();
        messages
            .create_stream_with_response_and_options(params, options)
            .await
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

    /// Rust equivalent of TS high-level `AnthropicFoundry.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Foundry message stream returning response metadata.
    pub async fn stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        let messages = self.client.messages();
        messages
            .stream_with_response_and_options(params, options)
            .await
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

    /// Rust equivalent of TS `AnthropicFoundry.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &MessageCountTokensParams,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count Foundry message tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &MessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageTokensCount>, ApiError> {
        let messages = self.client.messages();
        messages
            .count_tokens_with_response_and_options(params, options)
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
}

/// Foundry beta namespace wrapper.
///
/// Maps to TS Foundry `beta`, preserving beta models/files/skills while
/// replacing beta messages with a batch-free wrapper.
pub struct FoundryBeta<'a> {
    client: &'a Anthropic,
}

impl<'a> FoundryBeta<'a> {
    pub fn messages(&self) -> FoundryBetaMessages<'a> {
        FoundryBetaMessages {
            client: self.client,
        }
    }

    pub fn models(&self) -> BetaModels<'a> {
        BetaModels::new(self.client)
    }

    pub fn files(&self) -> BetaFiles<'a> {
        BetaFiles::new(self.client)
    }

    pub fn skills(&self) -> Skills<'a> {
        Skills::new(self.client)
    }
}

/// Foundry beta Messages wrapper with the unsupported Batch API omitted.
pub struct FoundryBetaMessages<'a> {
    client: &'a Anthropic,
}

impl<'a> FoundryBetaMessages<'a> {
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

    /// Rust equivalent of TS `AnthropicFoundry.beta.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Foundry beta message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        let beta = self.client.beta();
        let messages = beta.messages();
        messages
            .create_with_response_and_options(params, options)
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

    /// Rust equivalent of TS `AnthropicFoundry.beta.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Foundry beta message returning parsed data plus raw response metadata/body.
    pub async fn parse_with_response_and_options<T>(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let beta = self.client.beta();
        let messages = beta.messages();
        messages
            .parse_with_response_and_options(params, options)
            .await
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

    /// Rust equivalent of TS `AnthropicFoundry.beta.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Foundry beta streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<BetaMessageStreamEvent>>, ApiError> {
        let beta = self.client.beta();
        let messages = beta.messages();
        messages
            .create_stream_with_response_and_options(params, options)
            .await
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

    /// Rust equivalent of TS high-level `AnthropicFoundry.beta.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Foundry beta message stream returning response metadata.
    pub async fn stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        let beta = self.client.beta();
        let messages = beta.messages();
        messages
            .stream_with_response_and_options(params, options)
            .await
    }

    pub fn tool_runner(&self, params: BetaToolRunnerParams) -> BetaToolRunner<'a> {
        BetaToolRunner::new_with_message_client(
            FoundryBetaMessageCreateClient {
                client: self.client,
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
            FoundryBetaMessageCreateClient {
                client: self.client,
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

    /// Rust equivalent of TS `AnthropicFoundry.beta.messages.countTokens(...).withResponse()`.
    pub async fn count_tokens_with_response(
        &self,
        params: &BetaMessageCountTokensParams,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        self.count_tokens_with_response_and_options(params, None)
            .await
    }

    /// Count Foundry beta-message tokens returning parsed data plus raw response metadata/body.
    pub async fn count_tokens_with_response_and_options(
        &self,
        params: &BetaMessageCountTokensParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageTokensCount>, ApiError> {
        let beta = self.client.beta();
        let messages = beta.messages();
        messages
            .count_tokens_with_response_and_options(params, options)
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
}

struct FoundryBetaMessageCreateClient<'a> {
    client: &'a Anthropic,
}

impl BetaMessageCreateClient for FoundryBetaMessageCreateClient<'_> {
    fn create_beta_message_with_options<'b>(
        &'b self,
        params: &'b BetaMessageCreateParams,
        options: Option<&'b RequestOptions>,
    ) -> futures::future::BoxFuture<'b, Result<BetaMessage, ApiError>> {
        Box::pin(async move {
            let beta = self.client.beta();
            let messages = beta.messages();
            messages.create_with_options(params, options).await
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

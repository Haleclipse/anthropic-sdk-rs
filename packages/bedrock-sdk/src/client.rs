// Maps to: TS packages/bedrock-sdk/src/client.ts
//
// Amazon Bedrock provider for the Anthropic SDK. Constructs a client whose
// `base_url` points at the regional Bedrock Runtime endpoint and exposes a
// URL-rewriting helper that maps the standard `/v1/messages` path to the
// Bedrock model-invoke path.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use aws_credential_types::provider::ProvideCredentials;

use crate::core::streaming::BedrockEventStream;
use anthropic_sdk::client::{Anthropic, ClientOptions as CoreClientOptions};
use anthropic_sdk::core::error::ApiError;
use anthropic_sdk::core::response::{ApiResponse, RawResponse};
use anthropic_sdk::resources::beta::messages::{
    BetaMessage, BetaMessageCreateParams, BetaMessageStreamEvent,
};
use anthropic_sdk::resources::beta::{
    files::Files as BetaFiles, models::BetaModels, skills::Skills,
};
use anthropic_sdk::resources::completions::{Completion, CompletionCreateParams};
use anthropic_sdk::resources::messages::{Message, MessageCreateParams, MessageStreamEvent};
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
use anthropic_sdk::{HttpMiddleware, RequestOptions};

pub use anthropic_sdk::BaseAnthropic;

use crate::core::auth::{get_auth_headers, AwsCredentials};

/// Bedrock-specific API version header value.
/// Maps to: TS AnthropicBedrock.ANTHROPIC_VERSION
pub const ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";

// ─────────────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────────────

/// Async AWS credential provider for custom provider-chain integrations.
///
/// Maps to TS `providerChainResolver`: callers can plug in AWS SDK, SSO,
/// instance-profile, or other credential chains externally and return concrete
/// SigV4 credential material for each request.
pub trait AwsCredentialProvider: Send + Sync {
    /// Return AWS credentials to sign the next Bedrock Runtime request.
    fn get_credentials(&self) -> futures::future::BoxFuture<'_, Result<AwsCredentials, ApiError>>;
}

/// Configuration for the Amazon Bedrock provider.
///
/// Maps to: TS `AnthropicBedrock` constructor options.
///
/// AWS credentials are resolved in the following order:
///   1. Explicit fields on `BedrockConfig`.
///   2. Optional [`AwsCredentialProvider`] for custom provider-chain
///      resolution.
///   3. The AWS SDK for Rust default provider chain (`aws-config`), matching
///      the TS SDK's default `@aws-sdk/credential-providers` behavior.
///
/// Resolved credentials are fed into [`crate::core::auth::get_auth_headers`],
/// which signs a prepared Bedrock Runtime request with AWS SigV4.
#[derive(Clone)]
pub struct BedrockConfig {
    /// AWS region where the Bedrock endpoint lives (e.g. `us-east-1`).
    pub aws_region: String,

    /// AWS access key id. Optional when instance-profile / SSO credentials are
    /// available at a higher layer.
    pub aws_access_key: Option<String>,

    /// AWS secret access key.
    pub aws_secret_key: Option<String>,

    /// Temporary session token for assumed-role / SSO flows.
    pub aws_session_token: Option<String>,

    /// Optional endpoint override.
    /// Maps to TS `ClientOptions.baseURL` / `ANTHROPIC_BEDROCK_BASE_URL`.
    pub base_url: Option<String>,

    /// Optional custom AWS credential provider chain.
    /// Maps to TS `providerChainResolver`.
    pub credential_provider: Option<Arc<dyn AwsCredentialProvider>>,

    /// Skip SigV4 authentication for local proxies/tests.
    /// Maps to TS `skipAuth`.
    pub skip_auth: bool,
}

/// TS export-name compatibility alias for Bedrock constructor options.
pub type ClientOptions = BedrockConfig;

impl std::fmt::Debug for BedrockConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BedrockConfig")
            .field("aws_region", &self.aws_region)
            .field(
                "aws_access_key",
                &self.aws_access_key.as_ref().map(|_| "***"),
            )
            .field(
                "aws_secret_key",
                &self.aws_secret_key.as_ref().map(|_| "***"),
            )
            .field(
                "aws_session_token",
                &self.aws_session_token.as_ref().map(|_| "***"),
            )
            .field("base_url", &self.base_url)
            .field(
                "credential_provider",
                &self
                    .credential_provider
                    .as_ref()
                    .map(|_| "Some(<dyn AwsCredentialProvider>)"),
            )
            .field("skip_auth", &self.skip_auth)
            .finish()
    }
}

impl BedrockConfig {
    /// Build a config from environment variables.
    ///
    /// Maps to: TS `AnthropicBedrock` constructor reading `process.env`.
    ///
    /// Falls back to `"us-east-1"` when `AWS_REGION` is unset.
    ///
    /// AWS access-key environment variables are intentionally not copied into
    /// the config. Like the TS SDK, Rust lets the AWS SDK default provider
    /// chain resolve them so profile/SSO/IMDS precedence stays with the AWS
    /// SDK implementation.
    pub fn from_env() -> Self {
        let aws_region = std::env::var("AWS_REGION")
            .ok()
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| "us-east-1".to_owned());

        let base_url = std::env::var("ANTHROPIC_BEDROCK_BASE_URL")
            .ok()
            .map(|value| value.trim().to_owned());

        Self {
            aws_region,
            aws_access_key: None,
            aws_secret_key: None,
            aws_session_token: None,
            base_url,
            credential_provider: None,
            skip_auth: false,
        }
    }
}

#[derive(Clone)]
struct BedrockSigningMiddleware {
    config: BedrockConfig,
}

impl HttpMiddleware for BedrockSigningMiddleware {
    fn before_request<'a>(
        &'a self,
        request: &'a mut reqwest::Request,
    ) -> futures::future::BoxFuture<'a, Result<(), ApiError>> {
        Box::pin(async move { prepare_and_sign_bedrock_request(request, &self.config).await })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Client constructor
// ─────────────────────────────────────────────────────────────────────────────

/// Rust provider wrapper matching the TS `AnthropicBedrock` class name.
#[derive(Debug, Clone)]
pub struct AnthropicBedrock {
    inner: Anthropic,
    config: BedrockConfig,
}

impl AnthropicBedrock {
    /// Create an [`AnthropicBedrock`] client wrapper.
    pub fn new(config: BedrockConfig) -> Result<Self, ApiError> {
        Ok(Self {
            inner: create_client(config.clone())?,
            config,
        })
    }

    /// Create an [`AnthropicBedrock`] wrapper while also passing core SDK
    /// options such as timeout, retry count, default headers/query, logger, or
    /// custom HTTP client.
    ///
    /// This is the Rust equivalent of the TS provider `ClientOptions` type
    /// extending the core client options while omitting Anthropic API-key auth.
    pub fn new_with_core_options(
        config: BedrockConfig,
        core_options: CoreClientOptions,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            inner: create_client_with_core_options(config.clone(), core_options)?,
            config,
        })
    }

    /// Borrow the underlying core client.
    pub fn as_client(&self) -> &Anthropic {
        &self.inner
    }

    /// Maps to TS `AnthropicBedrock.messages`.
    pub fn messages(&self) -> BedrockMessages<'_> {
        BedrockMessages {
            client: &self.inner,
            config: &self.config,
        }
    }

    /// Maps to TS `AnthropicBedrock.completions`.
    pub fn completions(&self) -> BedrockCompletions<'_> {
        BedrockCompletions {
            client: &self.inner,
            config: &self.config,
        }
    }

    /// Maps to TS `AnthropicBedrock.beta`.
    pub fn beta(&self) -> BedrockBeta<'_> {
        BedrockBeta {
            client: &self.inner,
            config: &self.config,
        }
    }

    /// Consume the provider wrapper and return the underlying core client.
    pub fn into_inner(self) -> Anthropic {
        self.inner
    }
}

impl Deref for AnthropicBedrock {
    type Target = Anthropic;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Create an [`Anthropic`] client pre-configured for Amazon Bedrock.
///
/// Maps to: TS `new AnthropicBedrock(opts)` — sets the `base_url` to the
/// regional Bedrock Runtime HTTPS endpoint.
pub fn create_client(config: BedrockConfig) -> Result<Anthropic, ApiError> {
    create_client_with_core_options(config, CoreClientOptions::default())
}

/// Create an [`Anthropic`] Bedrock client while preserving caller-supplied core
/// client options (timeout, retries, default headers/query, logger, custom HTTP
/// client, etc.).
///
/// Maps to the TS Bedrock `ClientOptions` type, which is the provider-specific
/// option set plus `Omit<CoreClientOptions, 'apiKey' | 'authToken'>`. Rust keeps
/// provider config and core options as separate structs for backwards
/// compatibility with existing `BedrockConfig` literals.
pub fn create_client_with_core_options(
    config: BedrockConfig,
    mut core_options: CoreClientOptions,
) -> Result<Anthropic, ApiError> {
    let base_url = match config.base_url.as_ref() {
        // The TS provider uses a nullish default before calling the core
        // constructor; an explicit empty string then follows the core
        // `baseURL || https://api.anthropic.com` fallback.
        Some(base_url) if base_url.is_empty() => "https://api.anthropic.com".to_owned(),
        Some(base_url) => base_url.clone(),
        None => format!(
            "https://bedrock-runtime.{}.amazonaws.com",
            config.aws_region
        ),
    };

    // Bedrock uses SigV4 rather than Anthropic API-key auth. Omit x-api-key so
    // the core client doesn't reject requests before provider auth is applied.
    // Caller default headers are merged afterwards, matching TS defaultHeaders
    // behavior for provider clients.
    let mut default_headers = HashMap::new();
    default_headers.insert("x-api-key".to_owned(), None);
    if let Some(user_headers) = core_options.default_headers.take() {
        default_headers.extend(user_headers);
    }

    core_options.base_url = Some(base_url);
    core_options.api_key = None;
    core_options.auth_token = None;
    core_options.auth_token_provider = None;
    core_options.default_headers = Some(default_headers);
    // The official provider signs every request in prepareRequest(), including
    // inherited beta resources. Wrapper message/completion calls already carry
    // a SigV4 header and are left unchanged by this final middleware; inherited
    // resources are signed here instead of escaping through an unsigned core
    // client.
    core_options
        .middlewares
        .push(Arc::new(BedrockSigningMiddleware {
            config: config.clone(),
        }));

    Anthropic::new(core_options)
}

// ─────────────────────────────────────────────────────────────────────────────
// URL rewriting
// ─────────────────────────────────────────────────────────────────────────────

/// Rewrite a standard Anthropic API path to the Bedrock model-invocation URL.
///
/// Maps to: TS Bedrock URL rewrite: `/v1/messages` -> `/model/{model}/invoke`
/// (or `/invoke-with-response-stream` when `stream` is true).
pub fn rewrite_url(_path: &str, model: &str, stream: bool) -> String {
    let encoded_model = anthropic_sdk::internal::path::encode_uri_path(model);
    if stream {
        format!("/model/{encoded_model}/invoke-with-response-stream")
    } else {
        format!("/model/{encoded_model}/invoke")
    }
}

/// Bedrock Messages resource wrapper.
///
/// Maps to TS Bedrock `messages`, where model requests are rewritten from
/// `/v1/messages` to `/model/{model}/invoke` and `anthropic_version` is moved
/// into the JSON body.
pub struct BedrockMessages<'a> {
    client: &'a Anthropic,
    config: &'a BedrockConfig,
}

impl<'a> BedrockMessages<'a> {
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

    /// Rust equivalent of TS `AnthropicBedrock.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<Message>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Bedrock message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Message>, ApiError> {
        let rewrite_model_endpoint = bedrock_should_rewrite_model_endpoint("/v1/messages", options);
        let body = bedrock_message_body(params, false, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/messages", &model, false, options);
        let core_options = bedrock_core_options(options);
        let auth_headers = self
            .auth_headers_for_body("POST", &path, &body, core_options.as_ref())
            .await?;
        self.client
            .post_with_response(&path, &body, auth_headers.as_ref(), core_options.as_ref())
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

    /// Rust equivalent of TS `AnthropicBedrock.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<ParsedMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Bedrock message returning parsed data plus raw response metadata/body.
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
    ) -> Result<BedrockEventStream<MessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    pub async fn create_stream_with_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BedrockEventStream<MessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicBedrock.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<BedrockEventStream<MessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Bedrock streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &MessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BedrockEventStream<MessageStreamEvent>>, ApiError> {
        let rewrite_model_endpoint = bedrock_should_rewrite_model_endpoint("/v1/messages", options);
        let body = bedrock_message_body(params, true, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/messages", &model, true, options);
        let core_options = bedrock_core_options(options);
        let auth_headers = self
            .auth_headers_for_body("POST", &path, &body, core_options.as_ref())
            .await?;
        let response = self
            .client
            .post_stream_with_options(&path, &body, auth_headers.as_ref(), core_options.as_ref())
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: BedrockEventStream::new(response),
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

    /// Rust equivalent of TS high-level `AnthropicBedrock.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &MessageCreateParams,
    ) -> Result<ApiResponse<MessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Bedrock message stream returning response metadata.
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

    async fn auth_headers_for_body(
        &self,
        method: &str,
        path: &str,
        body: &serde_json::Value,
        options: Option<&RequestOptions>,
    ) -> Result<Option<HashMap<String, Option<String>>>, ApiError> {
        bedrock_auth_headers_for_body(self.client, self.config, method, path, body, options).await
    }
}

fn bedrock_message_body(
    params: &MessageCreateParams,
    _stream: bool,
    options: Option<&RequestOptions>,
    rewrite_model_endpoint: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = request_body_or_params(params, options, "MessageCreateParams")?;
    prepare_bedrock_model_body(&mut body, rewrite_model_endpoint)?;
    insert_bedrock_anthropic_beta_from_effective_header(&mut body, None, options)?;
    Ok(body)
}

/// Bedrock legacy Completions resource wrapper.
///
/// Maps to TS Bedrock `completions`, where `/v1/complete` is rewritten to the
/// model invocation endpoint and `anthropic_version` is moved into the body.
pub struct BedrockCompletions<'a> {
    client: &'a Anthropic,
    config: &'a BedrockConfig,
}

impl<'a> BedrockCompletions<'a> {
    pub async fn create(&self, params: &CompletionCreateParams) -> Result<Completion, ApiError> {
        self.create_with_options(params, None).await
    }

    pub async fn create_with_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<Completion, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicBedrock.completions.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<ApiResponse<Completion>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Bedrock completion returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Completion>, ApiError> {
        let rewrite_model_endpoint = bedrock_should_rewrite_model_endpoint("/v1/complete", options);
        let body = bedrock_completion_body(params, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/complete", &model, false, options);
        let core_options = bedrock_core_options(options);
        let headers = self
            .headers_for_body(
                "POST",
                &path,
                &body,
                params.betas.as_ref(),
                core_options.as_ref(),
            )
            .await?;
        self.client
            .post_with_response(&path, &body, headers.as_ref(), core_options.as_ref())
            .await
    }

    pub async fn create_stream(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<BedrockEventStream<Completion>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    pub async fn create_stream_with_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BedrockEventStream<Completion>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicBedrock.completions.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<ApiResponse<BedrockEventStream<Completion>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Bedrock streaming completion returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BedrockEventStream<Completion>>, ApiError> {
        let rewrite_model_endpoint = bedrock_should_rewrite_model_endpoint("/v1/complete", options);
        let body = bedrock_completion_body(params, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/complete", &model, true, options);
        let core_options = bedrock_core_options(options);
        let headers = self
            .headers_for_body(
                "POST",
                &path,
                &body,
                params.betas.as_ref(),
                core_options.as_ref(),
            )
            .await?;
        let response = self
            .client
            .post_stream_with_options(&path, &body, headers.as_ref(), core_options.as_ref())
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: BedrockEventStream::new(response),
            response: raw,
            request_id,
        })
    }

    async fn headers_for_body(
        &self,
        method: &str,
        path: &str,
        body: &serde_json::Value,
        betas: Option<&Vec<String>>,
        options: Option<&RequestOptions>,
    ) -> Result<Option<HashMap<String, Option<String>>>, ApiError> {
        let mut headers =
            bedrock_auth_headers_for_body(self.client, self.config, method, path, body, options)
                .await?
                .unwrap_or_default();
        if let Some(betas) = betas {
            if !betas.is_empty() {
                headers.insert("anthropic-beta".to_owned(), Some(betas.join(",")));
            }
        }
        Ok(if headers.is_empty() {
            None
        } else {
            Some(headers)
        })
    }
}

/// Bedrock beta namespace wrapper.
pub struct BedrockBeta<'a> {
    client: &'a Anthropic,
    config: &'a BedrockConfig,
}

impl<'a> BedrockBeta<'a> {
    /// Maps to TS `AnthropicBedrock.beta.messages`.
    pub fn messages(&self) -> BedrockBetaMessages<'a> {
        BedrockBetaMessages {
            client: self.client,
            config: self.config,
        }
    }

    /// Maps to TS `AnthropicBedrock.beta.models`.
    pub fn models(&self) -> BetaModels<'a> {
        BetaModels::new(self.client)
    }

    /// Maps to TS `AnthropicBedrock.beta.files`.
    pub fn files(&self) -> BetaFiles<'a> {
        BetaFiles::new(self.client)
    }

    /// Maps to TS `AnthropicBedrock.beta.skills`.
    pub fn skills(&self) -> Skills<'a> {
        Skills::new(self.client)
    }
}

/// Bedrock beta Messages wrapper.
///
/// Maps to TS Bedrock beta `messages`, where `/v1/messages?beta=true` is
/// rewritten to the Bedrock invoke endpoint and `anthropic_beta` is moved into
/// the JSON body from the beta header values.
pub struct BedrockBetaMessages<'a> {
    client: &'a Anthropic,
    config: &'a BedrockConfig,
}

impl<'a> BedrockBetaMessages<'a> {
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

    /// Rust equivalent of TS `AnthropicBedrock.beta.messages.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Bedrock beta message returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessage>, ApiError> {
        let rewrite_model_endpoint =
            bedrock_should_rewrite_model_endpoint("/v1/messages?beta=true", options);
        let body = bedrock_beta_message_body(params, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/messages?beta=true", &model, false, options);
        let core_options = bedrock_core_options(options);
        let headers = self
            .headers_for_body(
                "POST",
                &path,
                &body,
                params.betas.as_ref(),
                core_options.as_ref(),
            )
            .await?;
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

    /// Rust equivalent of TS `AnthropicBedrock.beta.messages.parse(...).withResponse()`.
    pub async fn parse_with_response<T>(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<ParsedBetaMessage<T>>, ApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.parse_with_response_and_options(params, None).await
    }

    /// Parse a Bedrock beta message returning parsed data plus raw response metadata/body.
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
    ) -> Result<BedrockEventStream<BetaMessageStreamEvent>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    pub async fn create_stream_with_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BedrockEventStream<BetaMessageStreamEvent>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `AnthropicBedrock.beta.messages.create({ stream: true }).withResponse()`.
    pub async fn create_stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BedrockEventStream<BetaMessageStreamEvent>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a Bedrock beta streaming message returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &BetaMessageCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BedrockEventStream<BetaMessageStreamEvent>>, ApiError> {
        let rewrite_model_endpoint =
            bedrock_should_rewrite_model_endpoint("/v1/messages?beta=true", options);
        let body = bedrock_beta_message_body(params, options, rewrite_model_endpoint)?;
        let model = bedrock_model_from_options_body(options, &params.model);
        let path = bedrock_final_path("/v1/messages?beta=true", &model, true, options);
        let core_options = bedrock_core_options(options);
        let headers = self
            .headers_for_body(
                "POST",
                &path,
                &body,
                params.betas.as_ref(),
                core_options.as_ref(),
            )
            .await?;
        let response = self
            .client
            .post_stream_with_options(&path, &body, headers.as_ref(), core_options.as_ref())
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: BedrockEventStream::new(response),
            response: raw,
            request_id,
        })
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

    /// Rust equivalent of TS high-level `AnthropicBedrock.beta.messages.stream(...).withResponse()`.
    pub async fn stream_with_response(
        &self,
        params: &BetaMessageCreateParams,
    ) -> Result<ApiResponse<BetaMessageStream>, ApiError> {
        self.stream_with_response_and_options(params, None).await
    }

    /// Create a high-level Bedrock beta message stream returning response metadata.
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
            BedrockBetaMessageCreateClient {
                client: self.client,
                config: self.config,
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
            BedrockBetaMessageCreateClient {
                client: self.client,
                config: self.config,
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

    async fn headers_for_body(
        &self,
        method: &str,
        path: &str,
        body: &serde_json::Value,
        betas: Option<&Vec<String>>,
        options: Option<&RequestOptions>,
    ) -> Result<Option<HashMap<String, Option<String>>>, ApiError> {
        let mut headers =
            bedrock_auth_headers_for_body(self.client, self.config, method, path, body, options)
                .await?
                .unwrap_or_default();
        if let Some(betas) = betas {
            if !betas.is_empty() {
                headers.insert("anthropic-beta".to_owned(), Some(betas.join(",")));
            }
        }
        Ok(if headers.is_empty() {
            None
        } else {
            Some(headers)
        })
    }
}

struct BedrockBetaMessageCreateClient<'a> {
    client: &'a Anthropic,
    config: &'a BedrockConfig,
}

impl BetaMessageCreateClient for BedrockBetaMessageCreateClient<'_> {
    fn create_beta_message_with_options<'b>(
        &'b self,
        params: &'b BetaMessageCreateParams,
        options: Option<&'b RequestOptions>,
    ) -> futures::future::BoxFuture<'b, Result<BetaMessage, ApiError>> {
        Box::pin(async move {
            BedrockBetaMessages {
                client: self.client,
                config: self.config,
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

fn bedrock_beta_message_body(
    params: &BetaMessageCreateParams,
    options: Option<&RequestOptions>,
    rewrite_model_endpoint: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = request_body_or_params(params, options, "BetaMessageCreateParams")?;
    prepare_bedrock_model_body(&mut body, rewrite_model_endpoint)?;
    insert_bedrock_anthropic_beta_from_effective_header(&mut body, params.betas.as_ref(), options)?;
    Ok(body)
}

fn bedrock_completion_body(
    params: &CompletionCreateParams,
    options: Option<&RequestOptions>,
    rewrite_model_endpoint: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = request_body_or_params(params, options, "CompletionCreateParams")?;
    prepare_bedrock_model_body(&mut body, rewrite_model_endpoint)?;
    insert_bedrock_anthropic_beta_from_effective_header(&mut body, params.betas.as_ref(), options)?;
    Ok(body)
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

fn bedrock_model_from_options_body(options: Option<&RequestOptions>, fallback: &str) -> String {
    options
        .and_then(|options| options.body.as_ref())
        .and_then(|body| body.get("model"))
        .and_then(|model| model.as_str())
        .unwrap_or(fallback)
        .to_owned()
}

fn bedrock_core_options(options: Option<&RequestOptions>) -> Option<RequestOptions> {
    options.cloned().map(|mut options| {
        // Provider wrappers have already applied RequestOptions.body/path before
        // URL rewrite/signing. Clear them before delegating to the core client
        // so the transformed Bedrock JSON body/path are sent on the wire.
        options.body = None;
        options.path = None;
        options
    })
}

fn bedrock_effective_resource_path<'a>(
    default_path: &'a str,
    options: Option<&'a RequestOptions>,
) -> &'a str {
    options
        .and_then(|options| options.path.as_deref())
        .unwrap_or(default_path)
}

fn bedrock_effective_method<'a>(
    default_method: &'a str,
    options: Option<&'a RequestOptions>,
) -> &'a str {
    options
        .and_then(|options| options.method.as_ref())
        .map(|method| method.as_str())
        .unwrap_or(default_method)
}

fn bedrock_should_rewrite_model_endpoint(
    default_path: &str,
    options: Option<&RequestOptions>,
) -> bool {
    bedrock_effective_method("POST", options).eq_ignore_ascii_case("POST")
        && matches!(
            bedrock_effective_resource_path(default_path, options),
            "/v1/messages" | "/v1/messages?beta=true" | "/v1/complete"
        )
}

fn bedrock_final_path(
    default_path: &str,
    model: &str,
    stream: bool,
    options: Option<&RequestOptions>,
) -> String {
    if bedrock_should_rewrite_model_endpoint(default_path, options) {
        rewrite_url(default_path, model, stream)
    } else {
        bedrock_effective_resource_path(default_path, options).to_owned()
    }
}

fn insert_bedrock_anthropic_beta_from_effective_header(
    body: &mut serde_json::Value,
    betas: Option<&Vec<String>>,
    options: Option<&RequestOptions>,
) -> Result<(), ApiError> {
    let mut beta_header =
        betas.and_then(|betas| (!betas.is_empty()).then(|| Some(betas.join(","))));

    if let Some(headers) = options.and_then(|options| options.headers.as_ref()) {
        if let Some(value) = headers
            .iter()
            .find_map(|(name, value)| name.eq_ignore_ascii_case("anthropic-beta").then_some(value))
        {
            beta_header = Some(value.clone());
        }
    }

    let Some(Some(beta_header)) = beta_header else {
        return Ok(());
    };

    let betas = beta_header
        .split(',')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    insert_bedrock_anthropic_beta(body, &betas)
}

fn insert_bedrock_anthropic_beta(
    body: &mut serde_json::Value,
    betas: &[String],
) -> Result<(), ApiError> {
    if betas.is_empty() {
        return Ok(());
    }
    let Some(obj) = body.as_object_mut() else {
        // TS Bedrock only moves anthropic-beta into the body when
        // `isObj(options.body)` is true; custom-path primitive/falsy bodies are
        // passed through to the core builder unchanged.
        return Ok(());
    };
    if obj
        .get("anthropic_beta")
        .is_some_and(json_value_is_js_truthy)
    {
        return Ok(());
    }
    obj.insert(
        "anthropic_beta".to_owned(),
        serde_json::Value::Array(
            betas
                .iter()
                .cloned()
                .map(serde_json::Value::String)
                .collect(),
        ),
    );
    Ok(())
}

fn prepare_bedrock_model_body(
    body: &mut serde_json::Value,
    rewrite_model_endpoint: bool,
) -> Result<(), ApiError> {
    let Some(obj) = body.as_object_mut() else {
        if rewrite_model_endpoint {
            return Err(ApiError::Sdk(
                "Expected request body to be an object for post /v1/messages".to_owned(),
            ));
        }
        // TS only injects anthropic_version into object bodies; custom paths
        // with primitive/falsy RequestOptions.body values pass through.
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
    if rewrite_model_endpoint {
        obj.remove("model");
        obj.remove("stream");
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

fn bedrock_wire_body_bytes(body: &serde_json::Value) -> Result<Vec<u8>, ApiError> {
    if !json_value_is_js_truthy(body) {
        return Ok(Vec::new());
    }
    serde_json::to_vec(body)
        .map_err(|err| ApiError::Sdk(format!("failed to serialize Bedrock body: {err}")))
}

async fn prepare_and_sign_bedrock_request(
    request: &mut reqwest::Request,
    config: &BedrockConfig,
) -> Result<(), ApiError> {
    // Provider wrappers sign explicitly while constructing their transformed
    // request. Avoid resolving credentials twice for those requests.
    if request
        .headers()
        .get(reqwest::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("AWS4-HMAC-SHA256 "))
    {
        return Ok(());
    }

    let body = prepare_bedrock_middleware_body(request)?;
    if config.skip_auth {
        return Ok(());
    }
    if config.aws_region.is_empty() {
        return Err(ApiError::Sdk(
            "Expected `awsRegion` option to be passed to the client or the `AWS_REGION` environment variable to be present"
                .to_owned(),
        ));
    }

    let mut resolved_config = config.clone();
    match (&config.aws_access_key, &config.aws_secret_key) {
        (Some(_), Some(_)) => {}
        (None, None) => {
            let credentials = resolve_bedrock_credentials(config).await?;
            resolved_config.aws_access_key = Some(credentials.access_key_id);
            resolved_config.aws_secret_key = Some(credentials.secret_access_key);
            resolved_config.aws_session_token = credentials.session_token;
            resolved_config.credential_provider = None;
        }
        _ => {
            // Preserve the explicit partial-credential validation produced by
            // the signer rather than silently falling back to another source.
        }
    }

    let headers = get_auth_headers(
        request.method().as_str(),
        request.url().as_str(),
        &body,
        &resolved_config,
    )?;
    for (name, value) in headers {
        if name == "host" {
            continue;
        }
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            ApiError::Sdk(format!(
                "invalid Bedrock signing header name '{name}': {error}"
            ))
        })?;
        let value = reqwest::header::HeaderValue::from_str(&value).map_err(|error| {
            ApiError::Sdk(format!("invalid Bedrock signing header value: {error}"))
        })?;
        request.headers_mut().insert(name, value);
    }
    Ok(())
}

fn prepare_bedrock_middleware_body(request: &mut reqwest::Request) -> Result<Vec<u8>, ApiError> {
    let Some(body) = request.body() else {
        return Ok(Vec::new());
    };
    let Some(bytes) = body.as_bytes() else {
        return Err(ApiError::Sdk(
            "Unable to SigV4-sign a streaming Bedrock request body".to_owned(),
        ));
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }

    let mut bytes = bytes.to_vec();
    let is_json = request
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(';').next() == Some("application/json"));
    if !is_json {
        return Ok(bytes);
    }

    let mut body: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        ApiError::Sdk(format!(
            "failed to parse prepared Bedrock JSON request body: {error}"
        ))
    })?;
    let Some(object) = body.as_object_mut() else {
        return Ok(bytes);
    };
    if !object
        .get("anthropic_version")
        .is_some_and(json_value_is_js_truthy)
    {
        object.insert(
            "anthropic_version".to_owned(),
            serde_json::Value::String(ANTHROPIC_VERSION.to_owned()),
        );
    }
    if !object
        .get("anthropic_beta")
        .is_some_and(json_value_is_js_truthy)
    {
        if let Some(beta_header) = request
            .headers()
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok())
        {
            object.insert(
                "anthropic_beta".to_owned(),
                serde_json::Value::Array(
                    beta_header
                        .split(',')
                        .map(|value| serde_json::Value::String(value.to_owned()))
                        .collect(),
                ),
            );
        }
    }

    bytes = serde_json::to_vec(&body).map_err(|error| {
        ApiError::Sdk(format!(
            "failed to serialize prepared Bedrock JSON request body: {error}"
        ))
    })?;
    *request.body_mut() = Some(bytes.clone().into());
    request
        .headers_mut()
        .remove(reqwest::header::CONTENT_LENGTH);
    Ok(bytes)
}

async fn resolve_bedrock_credentials(config: &BedrockConfig) -> Result<AwsCredentials, ApiError> {
    if let Some(provider) = &config.credential_provider {
        return provider.get_credentials().await;
    }

    let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(config.aws_region.clone()))
        .load()
        .await;
    let provider = sdk_config.credentials_provider().ok_or_else(|| {
        ApiError::Sdk(
            "Failed to resolve AWS credentials from default provider chain: no credentials provider was configured"
                .to_owned(),
        )
    })?;
    let credentials = provider.provide_credentials().await.map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to resolve AWS credentials from default provider chain: {err}"
        ))
    })?;

    Ok(AwsCredentials {
        access_key_id: credentials.access_key_id().to_owned(),
        secret_access_key: credentials.secret_access_key().to_owned(),
        session_token: credentials.session_token().map(str::to_owned),
    })
}

async fn bedrock_auth_headers_for_body(
    client: &Anthropic,
    config: &BedrockConfig,
    method: &str,
    path: &str,
    body: &serde_json::Value,
    options: Option<&RequestOptions>,
) -> Result<Option<HashMap<String, Option<String>>>, ApiError> {
    if config.skip_auth {
        return Ok(None);
    }
    if config.aws_region.is_empty() {
        return Err(ApiError::Sdk(
            "Expected `awsRegion` option to be passed to the client or the `AWS_REGION` environment variable to be present"
                .to_owned(),
        ));
    }

    let credentials = match (&config.aws_access_key, &config.aws_secret_key) {
        (Some(_), Some(_)) => None,
        (None, None) => Some(resolve_bedrock_credentials(config).await?),
        _ => {
            // If the user supplied partial static credentials, surface the same
            // validation error as the signer instead of silently sending an
            // unsigned request.
            None
        }
    };

    let signing_config;
    let config_for_signing = if let Some(credentials) = credentials {
        signing_config = BedrockConfig {
            aws_region: config.aws_region.clone(),
            aws_access_key: Some(credentials.access_key_id),
            aws_secret_key: Some(credentials.secret_access_key),
            aws_session_token: credentials.session_token,
            base_url: config.base_url.clone(),
            credential_provider: None,
            skip_auth: false,
        };
        &signing_config
    } else {
        config
    };

    let effective_method = options
        .and_then(|options| options.method.as_ref())
        .map(|method| method.as_str())
        .unwrap_or(method);
    let url = client.build_url_with_default_base_url(
        path,
        options.and_then(|options| options.query.as_ref()),
        options.and_then(|options| options.default_base_url.as_deref()),
    )?;
    let body_bytes = if let Some(raw) = options.and_then(|options| options.raw_body.as_ref()) {
        raw.body.to_vec()
    } else {
        bedrock_wire_body_bytes(body)?
    };
    let headers = get_auth_headers(effective_method, &url, &body_bytes, config_for_signing)?;
    Ok(Some(
        headers
            .into_iter()
            // Let the HTTP stack set the wire Host header. For standard AWS
            // HTTPS endpoints this matches the signed host value; omitting it
            // also keeps local test servers with dynamic ports routable.
            .filter(|(key, _)| key != "host")
            .map(|(key, value)| (key, Some(value)))
            .collect(),
    ))
}

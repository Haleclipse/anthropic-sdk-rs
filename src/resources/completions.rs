// Maps to: TS resources/completions.ts

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::core::streaming::SseStream;
use crate::internal::request_options::RequestOptions;

// ─────────────────────────────────────────────────────────────────────────────
// Response type
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS Completion
///
/// A text-completion response from the legacy Text Completions API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// Unique object identifier.
    ///
    /// The format and length of IDs may change over time.
    pub id: String,

    /// The resulting completion up to and excluding the stop sequences.
    pub completion: String,

    /// The model that handled the completion.
    pub model: String,

    /// The reason that we stopped.
    ///
    /// May be `"stop_sequence"` or `"max_tokens"`.
    pub stop_reason: Option<String>,

    /// Object type. Always `"completion"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for Completion {
    fn default() -> Self {
        Self {
            id: String::new(),
            completion: String::new(),
            model: String::new(),
            stop_reason: None,
            type_name: "completion".to_owned(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Request params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS CompletionCreateParamsBase
///
/// Parameters for the legacy Text Completions API (`POST /v1/complete`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCreateParams {
    /// The maximum number of tokens to generate before stopping.
    pub max_tokens_to_sample: i64,

    /// The model that will complete your prompt.
    pub model: String,

    /// The prompt that you want Claude to complete.
    ///
    /// Must use alternating `\n\nHuman:` and `\n\nAssistant:` turns.
    pub prompt: String,

    /// An object describing metadata about the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::resources::messages::Metadata>,

    /// Sequences that will cause the model to stop generating.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,

    /// Whether to incrementally stream the response using server-sent events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    /// Amount of randomness injected into the response (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Only sample from the top K options for each subsequent token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,

    /// Use nucleus sampling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    ///
    /// Maps to: TS `CompletionCreateParamsBase.betas`; this is a header param
    /// and is never serialized into the JSON request body.
    #[serde(skip)]
    pub betas: Option<Vec<String>>,
}

/// TS namespace compatibility alias for `CompletionCreateParams.Metadata`.
pub type Metadata = crate::resources::messages::Metadata;

/// TS export-name compatibility alias for `CompletionCreateParamsBase`.
pub type CompletionCreateParamsBase = CompletionCreateParams;
/// TS export-name compatibility alias for `CompletionCreateParamsNonStreaming`.
pub type CompletionCreateParamsNonStreaming = CompletionCreateParams;
/// TS export-name compatibility alias for `CompletionCreateParamsStreaming`.
pub type CompletionCreateParamsStreaming = CompletionCreateParams;

fn completion_beta_headers(betas: Option<&[String]>) -> Option<HashMap<String, Option<String>>> {
    let value = betas?
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let mut headers = HashMap::new();
    headers.insert("anthropic-beta".to_owned(), Some(value));
    Some(headers)
}

fn completion_body(
    params: &CompletionCreateParams,
    stream: bool,
) -> Result<serde_json::Value, ApiError> {
    let mut body = serde_json::to_value(params)
        .map_err(|e| ApiError::Sdk(format!("failed to serialize CompletionCreateParams: {e}")))?;
    let obj = body.as_object_mut().ok_or_else(|| {
        ApiError::Sdk("CompletionCreateParams did not serialize to object".into())
    })?;
    obj.remove("betas");
    obj.insert("stream".into(), serde_json::Value::Bool(stream));
    Ok(body)
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS Completions class -- resource-based accessor for the legacy
/// Text Completions API.
///
/// Obtain an instance via [`Anthropic::completions`]:
///
/// ```ignore
/// let client = Anthropic::new(ClientOptions::default())?;
/// let completion = client.completions().create(&params).await?;
/// ```
pub struct Completions<'a> {
    client: &'a Anthropic,
}

impl<'a> Completions<'a> {
    /// Create a new `Completions` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Completions.create() -- POST /v1/complete
    ///
    /// [Legacy] Create a Text Completion.
    ///
    /// The Text Completions API is a legacy API. We recommend using the
    /// Messages API going forward.
    pub async fn create(&self, params: &CompletionCreateParams) -> Result<Completion, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Create a Text Completion with per-request options.
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

    /// Rust equivalent of TS `client.completions.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<ApiResponse<Completion>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a Text Completion returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Completion>, ApiError> {
        let body = completion_body(params, false)?;
        let headers = completion_beta_headers(params.betas.as_deref());

        self.client
            .post_with_response("/v1/complete", &body, headers.as_ref(), options)
            .await
    }

    /// Maps to: TS Completions.create() with stream:true -- POST /v1/complete
    ///
    /// [Legacy] Create a streaming Text Completion.
    ///
    /// Returns an [`SseStream`] that yields [`Completion`] events as they arrive.
    pub async fn create_stream(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<SseStream<Completion>, ApiError> {
        self.create_stream_with_options(params, None).await
    }

    /// Create a streaming Text Completion with per-request options.
    pub async fn create_stream_with_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<SseStream<Completion>, ApiError> {
        Ok(self
            .create_stream_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.completions.create({ stream: true }).withResponse()`.
    ///
    /// The raw response body remains owned by the returned SSE stream, so
    /// `response.body` is intentionally empty and only metadata/headers are
    /// captured.
    pub async fn create_stream_with_response(
        &self,
        params: &CompletionCreateParams,
    ) -> Result<ApiResponse<SseStream<Completion>>, ApiError> {
        self.create_stream_with_response_and_options(params, None)
            .await
    }

    /// Create a streaming Text Completion returning stream plus raw response metadata.
    pub async fn create_stream_with_response_and_options(
        &self,
        params: &CompletionCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<SseStream<Completion>>, ApiError> {
        let body = completion_body(params, true)?;
        let headers = completion_beta_headers(params.betas.as_deref());

        let response = self
            .client
            .post_stream_with_options("/v1/complete", &body, headers.as_ref(), options)
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);
        Ok(ApiResponse {
            data: SseStream::new(response),
            response: raw,
            request_id,
        })
    }
}

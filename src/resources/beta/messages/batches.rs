// Maps to: TS resources/beta/messages/batches.ts

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::core::streaming::JsonLineStream;
use crate::internal::path::encode_path_param;
use crate::internal::request_options::RequestOptions;
use crate::resources::messages::batches::MessageBatchProcessingStatus;

use super::types::{BetaMessage, BetaMessageCreateParams};
use crate::resources::beta::types::BetaErrorResponse;

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaMessageBatchRequestCounts
///
/// Tallies requests within the Message Batch, categorized by their status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageBatchRequestCounts {
    /// Number of canceled requests.
    pub canceled: i64,
    /// Number of errored requests.
    pub errored: i64,
    /// Number of expired requests.
    pub expired: i64,
    /// Number of processing requests.
    pub processing: i64,
    /// Number of succeeded requests.
    pub succeeded: i64,
}

/// Maps to: TS BetaMessageBatch
///
/// A message batch object returned by the beta Message Batches API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageBatch {
    /// Unique object identifier.
    pub id: String,

    /// RFC 3339 datetime when the batch was archived.
    pub archived_at: Option<String>,

    /// RFC 3339 datetime when cancellation was initiated.
    pub cancel_initiated_at: Option<String>,

    /// RFC 3339 datetime when the batch was created.
    pub created_at: String,

    /// RFC 3339 datetime when processing ended.
    pub ended_at: Option<String>,

    /// RFC 3339 datetime when the batch expires.
    pub expires_at: String,

    /// Processing status: `"in_progress"`, `"canceling"`, or `"ended"`.
    pub processing_status: MessageBatchProcessingStatus,

    /// Tallies of requests by status.
    pub request_counts: BetaMessageBatchRequestCounts,

    /// URL to the `.jsonl` results file.
    pub results_url: Option<String>,

    /// Object type. Always `"message_batch"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaMessageBatch {
    fn default() -> Self {
        Self {
            id: String::new(),
            archived_at: None,
            cancel_initiated_at: None,
            created_at: String::new(),
            ended_at: None,
            expires_at: String::new(),
            processing_status: MessageBatchProcessingStatus::InProgress,
            request_counts: BetaMessageBatchRequestCounts {
                canceled: 0,
                errored: 0,
                expired: 0,
                processing: 0,
                succeeded: 0,
            },
            results_url: None,
            type_name: "message_batch".to_owned(),
        }
    }
}

/// Maps to: TS BetaDeletedMessageBatch
///
/// Confirmation of a message batch deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaDeletedMessageBatch {
    /// ID of the Message Batch.
    pub id: String,

    /// Deleted object type. Always `"message_batch_deleted"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

impl Default for BetaDeletedMessageBatch {
    fn default() -> Self {
        Self {
            id: String::new(),
            type_name: "message_batch_deleted".to_owned(),
        }
    }
}

/// Maps to: TS BetaMessageBatchResult
///
/// Processing result for a single request in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaMessageBatchResult {
    /// Maps to: TS BetaMessageBatchSucceededResult
    #[serde(rename = "succeeded")]
    Succeeded { message: Box<BetaMessage> },

    /// Maps to: TS BetaMessageBatchErroredResult
    #[serde(rename = "errored")]
    Errored { error: BetaErrorResponse },

    /// Maps to: TS BetaMessageBatchCanceledResult
    #[serde(rename = "canceled")]
    Canceled,

    /// Maps to: TS BetaMessageBatchExpiredResult
    #[serde(rename = "expired")]
    Expired,
}

/// TS export-name compatibility alias for `BetaMessageBatchSucceededResult`.
pub type BetaMessageBatchSucceededResult = BetaMessageBatchResult;
/// TS export-name compatibility alias for `BetaMessageBatchErroredResult`.
pub type BetaMessageBatchErroredResult = BetaMessageBatchResult;
/// TS export-name compatibility alias for `BetaMessageBatchCanceledResult`.
pub type BetaMessageBatchCanceledResult = BetaMessageBatchResult;
/// TS export-name compatibility alias for `BetaMessageBatchExpiredResult`.
pub type BetaMessageBatchExpiredResult = BetaMessageBatchResult;

/// Maps to: TS BetaMessageBatchIndividualResponse
///
/// A single line in the batch results `.jsonl` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageBatchIndividualResponse {
    /// Developer-provided ID for matching results to requests.
    pub custom_id: String,

    /// Processing result for this request.
    pub result: BetaMessageBatchResult,
}

/// Maps to: TS BetaMessageBatchesPage (Page<BetaMessageBatch>)
///
/// A paginated list response for batch listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessageBatchesPage {
    /// The list of batches in this page.
    pub data: Vec<BetaMessageBatch>,

    /// Whether there are more pages after this one.
    #[serde(default)]
    pub has_more: bool,

    /// ID of the first item in this page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,

    /// ID of the last item in this page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Request params
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BatchCreateParams.Request
///
/// A single request within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBatchRequest {
    /// Developer-provided ID. Must be unique within the batch.
    pub custom_id: String,

    /// Messages API creation parameters for the individual request.
    pub params: BetaMessageCreateParams,
}

/// TS namespace compatibility alias for `BatchCreateParams.Request`.
pub type Request = BetaBatchRequest;
/// TS namespace compatibility alias for `BatchCreateParams.Request.Params`.
pub type Params = BetaMessageCreateParams;

/// Maps to: TS BatchCreateParams
///
/// Parameters for creating a message batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBatchCreateParams {
    /// List of requests for prompt completion.
    pub requests: Vec<BetaBatchRequest>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip)]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS BatchRetrieveParams
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaBatchRetrieveParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS BatchListParams
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaBatchListParams {
    /// ID of the item to use as a cursor for backward pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,

    /// ID of the item to use as a cursor for forward pagination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,

    /// Maximum number of items to return per page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,

    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS BatchDeleteParams
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaBatchDeleteParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS BatchCancelParams
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BetaBatchCancelParams {
    /// Optional beta version(s) to send in the `anthropic-beta` header.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub betas: Option<Vec<String>>,
}

/// Maps to: TS BatchResultsParams.
pub type BetaBatchResultsParams = BetaBatchRetrieveParams;

/// TS beta namespace export-name alias for `BatchCreateParams`.
pub type BatchCreateParams = BetaBatchCreateParams;
/// TS beta namespace export-name alias for `BatchRetrieveParams`.
pub type BatchRetrieveParams = BetaBatchRetrieveParams;
/// TS beta namespace export-name alias for `BatchListParams`.
pub type BatchListParams = BetaBatchListParams;
/// TS beta namespace export-name alias for `BatchDeleteParams`.
pub type BatchDeleteParams = BetaBatchDeleteParams;
/// TS beta namespace export-name alias for `BatchCancelParams`.
pub type BatchCancelParams = BetaBatchCancelParams;
/// TS beta namespace export-name alias for `BatchResultsParams`.
pub type BatchResultsParams = BetaBatchResultsParams;

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Build the `anthropic-beta` header value, always including the
/// `message-batches-2024-09-24` feature flag.
fn build_batch_beta_header(betas: Option<&[String]>) -> HashMap<String, Option<String>> {
    let mut parts: Vec<String> = betas.map(|b| b.to_vec()).unwrap_or_default();
    parts.push("message-batches-2024-09-24".to_owned());
    let mut h = HashMap::new();
    h.insert("anthropic-beta".to_owned(), Some(parts.join(",")));
    h
}

// ─────────────────────────────────────────────────────────────────────────────
// Resource
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS beta/messages Batches class
///
/// Resource for managing message batches through the beta API.
pub struct BetaBatches<'a> {
    client: &'a Anthropic,
}

/// TS beta namespace export-name alias for the `Batches` resource class.
pub type Batches<'a> = BetaBatches<'a>;

impl<'a> BetaBatches<'a> {
    /// Create a new `BetaBatches` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Batches.create() -- POST /v1/messages/batches?beta=true
    ///
    /// Send a batch of Message creation requests.
    pub async fn create(
        &self,
        params: &BetaBatchCreateParams,
    ) -> Result<BetaMessageBatch, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Create a beta message batch with per-request options.
    pub async fn create_with_options(
        &self,
        params: &BetaBatchCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageBatch, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BetaBatchCreateParams,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a beta message batch returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BetaBatchCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        let body = serde_json::to_value(params).map_err(|e| {
            ApiError::Sdk(format!("failed to serialize BetaBatchCreateParams: {e}"))
        })?;

        let headers = build_batch_beta_header(params.betas.as_deref());

        self.client
            .post_with_response(
                "/v1/messages/batches?beta=true",
                &body,
                Some(&headers),
                options,
            )
            .await
    }

    /// Maps to: TS Batches.retrieve() -- GET /v1/messages/batches/{id}?beta=true
    ///
    /// Retrieve a message batch by ID.
    pub async fn retrieve(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchRetrieveParams>,
    ) -> Result<BetaMessageBatch, ApiError> {
        self.retrieve_with_options(batch_id, params, None).await
    }

    /// Retrieve a beta message batch with per-request options.
    pub async fn retrieve_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageBatch, ApiError> {
        Ok(self
            .retrieve_with_response_and_options(batch_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.retrieve(...).withResponse()`.
    pub async fn retrieve_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchRetrieveParams>,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        self.retrieve_with_response_and_options(batch_id, params, None)
            .await
    }

    /// Retrieve a beta message batch returning parsed data plus raw response metadata/body.
    pub async fn retrieve_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchRetrieveParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        let batch_id = encode_path_param(batch_id)?;
        let path = format!("/v1/messages/batches/{batch_id}?beta=true");
        let headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .get_with_response(&path, None, Some(&headers), options)
            .await
    }

    /// Maps to: TS Batches.list() -- GET /v1/messages/batches?beta=true
    ///
    /// List all message batches.
    pub async fn list(
        &self,
        params: Option<&BetaBatchListParams>,
    ) -> Result<BetaMessageBatchesPage, ApiError> {
        self.list_with_options(params, None).await
    }

    /// List beta message batches with per-request options.
    pub async fn list_with_options(
        &self,
        params: Option<&BetaBatchListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageBatchesPage, ApiError> {
        Ok(self
            .list_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        params: Option<&BetaBatchListParams>,
    ) -> Result<ApiResponse<BetaMessageBatchesPage>, ApiError> {
        self.list_with_response_and_options(params, None).await
    }

    /// List beta message batches returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        params: Option<&BetaBatchListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageBatchesPage>, ApiError> {
        let mut query = HashMap::new();

        if let Some(p) = params {
            if let Some(ref before_id) = p.before_id {
                query.insert("before_id".to_owned(), Some(before_id.clone()));
            }
            if let Some(ref after_id) = p.after_id {
                query.insert("after_id".to_owned(), Some(after_id.clone()));
            }
            if let Some(limit) = p.limit {
                query.insert("limit".to_owned(), Some(limit.to_string()));
            }
        }

        let headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));
        let query_ref = if query.is_empty() { None } else { Some(&query) };

        self.client
            .get_with_response(
                "/v1/messages/batches?beta=true",
                query_ref,
                Some(&headers),
                options,
            )
            .await
    }

    /// Maps to: TS Batches.delete() -- DELETE /v1/messages/batches/{id}?beta=true
    ///
    /// Delete a message batch. Can only delete batches that have finished processing.
    pub async fn delete(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchDeleteParams>,
    ) -> Result<BetaDeletedMessageBatch, ApiError> {
        self.delete_with_options(batch_id, params, None).await
    }

    /// Delete a beta message batch with per-request options.
    pub async fn delete_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaDeletedMessageBatch, ApiError> {
        Ok(self
            .delete_with_response_and_options(batch_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.delete(...).withResponse()`.
    pub async fn delete_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchDeleteParams>,
    ) -> Result<ApiResponse<BetaDeletedMessageBatch>, ApiError> {
        self.delete_with_response_and_options(batch_id, params, None)
            .await
    }

    /// Delete a beta message batch returning parsed data plus raw response metadata/body.
    pub async fn delete_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchDeleteParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaDeletedMessageBatch>, ApiError> {
        let batch_id = encode_path_param(batch_id)?;
        let path = format!("/v1/messages/batches/{batch_id}?beta=true");
        let headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .delete_with_response(&path, Some(&headers), options)
            .await
    }

    /// Maps to: TS Batches.cancel() -- POST /v1/messages/batches/{id}/cancel?beta=true
    ///
    /// Cancel a message batch. Batches may be canceled any time before processing ends.
    pub async fn cancel(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchCancelParams>,
    ) -> Result<BetaMessageBatch, ApiError> {
        self.cancel_with_options(batch_id, params, None).await
    }

    /// Cancel a beta message batch with per-request options.
    pub async fn cancel_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchCancelParams>,
        options: Option<&RequestOptions>,
    ) -> Result<BetaMessageBatch, ApiError> {
        Ok(self
            .cancel_with_response_and_options(batch_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.cancel(...).withResponse()`.
    pub async fn cancel_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchCancelParams>,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        self.cancel_with_response_and_options(batch_id, params, None)
            .await
    }

    /// Cancel a beta message batch returning parsed data plus raw response metadata/body.
    pub async fn cancel_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchCancelParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<BetaMessageBatch>, ApiError> {
        let batch_id = encode_path_param(batch_id)?;
        let path = format!("/v1/messages/batches/{batch_id}/cancel?beta=true");
        let headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));

        self.client
            .request_with_response(
                reqwest::Method::POST,
                &path,
                None,
                Some(&headers),
                None,
                options,
            )
            .await
    }

    /// Rust convenience helper that buffers all TS `Batches.results()` items.
    ///
    /// Retrieves the batch, then fetches and parses the results as a `Vec`
    /// of [`BetaMessageBatchIndividualResponse`] items. Each item corresponds
    /// to one request in the original batch.
    ///
    /// Returns an error if the batch has not yet finished processing (i.e.
    /// `results_url` is `None`).
    pub async fn results_all(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<Vec<BetaMessageBatchIndividualResponse>, ApiError> {
        self.results_all_with_options(batch_id, params, None).await
    }

    /// Fetch beta batch results with per-request options.
    pub async fn results_all_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<Vec<BetaMessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_all_with_response_and_options(batch_id, params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.beta.messages.batches.results(...).withResponse()`.
    pub async fn results_all_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<ApiResponse<Vec<BetaMessageBatchIndividualResponse>>, ApiError> {
        self.results_all_with_response_and_options(batch_id, params, None)
            .await
    }

    /// Fetch beta batch results returning parsed JSONL items plus raw binary response metadata/body.
    pub async fn results_all_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Vec<BetaMessageBatchIndividualResponse>>, ApiError> {
        // TS beta `Batches.results()` retrieves batch metadata without
        // forwarding result-fetch params/options, then applies them only to the
        // binary results fetch below.
        let batch = self.retrieve(batch_id, None).await?;

        let results_url = batch.results_url.ok_or_else(|| {
            ApiError::Sdk(format!(
                "No batch `results_url`; Has it finished processing? {} - {}",
                batch.processing_status.as_str(),
                batch.id
            ))
        })?;

        let mut headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));
        headers.insert("accept".to_owned(), Some("application/binary".to_owned()));

        // Fetch the JSONL file as raw bytes like TS `__binaryResponse`, then
        // decode UTF-8 and parse line-by-line.
        let response = self
            .client
            .get_binary_with_response(&results_url, None, Some(&headers), options)
            .await?;
        let data = parse_beta_message_batch_results(&response.data)?;

        Ok(ApiResponse {
            data,
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to TS `client.beta.messages.batches.results()`.
    ///
    /// Exposes the JSONL response as a lazy [`JsonLineStream`].
    pub async fn results(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<JsonLineStream<BetaMessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_with_response_and_options(batch_id, params, None)
            .await?
            .data)
    }

    /// Streaming beta batch results with per-request options.
    pub async fn results_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<JsonLineStream<BetaMessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_with_response_and_options(batch_id, params, options)
            .await?
            .data)
    }

    /// Streaming beta batch results with raw response metadata.
    pub async fn results_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<ApiResponse<JsonLineStream<BetaMessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response_and_options(batch_id, params, None)
            .await
    }

    /// Streaming beta batch results with raw response metadata and per-request options.
    pub async fn results_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<JsonLineStream<BetaMessageBatchIndividualResponse>>, ApiError> {
        let batch = self.retrieve(batch_id, None).await?;

        let results_url = batch.results_url.ok_or_else(|| {
            ApiError::Sdk(format!(
                "No batch `results_url`; Has it finished processing? {} - {}",
                batch.processing_status.as_str(),
                batch.id
            ))
        })?;

        let mut headers = build_batch_beta_header(params.and_then(|p| p.betas.as_deref()));
        headers.insert("accept".to_owned(), Some("application/binary".to_owned()));

        let response = self
            .client
            .get_stream_with_options(&results_url, None, Some(&headers), options)
            .await?;
        let raw = RawResponse::from_response_metadata(&response);
        let request_id = raw.request_id().map(str::to_owned);

        Ok(ApiResponse {
            data: JsonLineStream::new(response.bytes_stream()),
            response: raw,
            request_id,
        })
    }

    /// Backward-compatible alias for [`BetaBatches::results`].
    pub async fn results_stream(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<JsonLineStream<BetaMessageBatchIndividualResponse>, ApiError> {
        self.results(batch_id, params).await
    }

    /// Backward-compatible alias for [`BetaBatches::results_with_options`].
    pub async fn results_stream_with_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<JsonLineStream<BetaMessageBatchIndividualResponse>, ApiError> {
        self.results_with_options(batch_id, params, options).await
    }

    /// Backward-compatible alias for [`BetaBatches::results_with_response`].
    pub async fn results_stream_with_response(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
    ) -> Result<ApiResponse<JsonLineStream<BetaMessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response(batch_id, params).await
    }

    /// Backward-compatible alias for [`BetaBatches::results_with_response_and_options`].
    pub async fn results_stream_with_response_and_options(
        &self,
        batch_id: &str,
        params: Option<&BetaBatchResultsParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<JsonLineStream<BetaMessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response_and_options(batch_id, params, options)
            .await
    }
}

fn parse_beta_message_batch_results(
    response_bytes: &[u8],
) -> Result<Vec<BetaMessageBatchIndividualResponse>, ApiError> {
    let response_text = String::from_utf8(response_bytes.to_vec())
        .map_err(|err| ApiError::Sdk(format!("beta batch results were not valid UTF-8: {err}")))?;
    let mut items = Vec::new();

    for line in response_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item: BetaMessageBatchIndividualResponse = serde_json::from_str(trimmed)
            .map_err(|e| ApiError::Sdk(format!("failed to parse beta batch result line: {e}")))?;
        items.push(item);
    }

    Ok(items)
}

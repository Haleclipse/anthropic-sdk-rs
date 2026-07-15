// Maps to: TS resources/messages/batches.ts
//
// Message Batches API -- create, retrieve, list, cancel, delete, and stream
// results for batch message processing jobs.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::core::response::{ApiResponse, RawResponse};
use crate::core::streaming::JsonLineStream;
use crate::internal::path::encode_path_param;
use crate::internal::request_options::RequestOptions;
use crate::resources::shared::ErrorResponse;

use super::types::{Message, MessageCreateParams};

// ==========================================================================
// Types
// ==========================================================================

// --------------------------------------------------------------------------
// MessageBatch
// --------------------------------------------------------------------------

/// Maps to: TS MessageBatch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatch {
    /// Unique object identifier.
    pub id: String,

    /// RFC 3339 datetime string representing the time at which the Message
    /// Batch was archived and its results became unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,

    /// RFC 3339 datetime string representing the time at which cancellation
    /// was initiated for the Message Batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_initiated_at: Option<String>,

    /// RFC 3339 datetime string representing the time at which the Message
    /// Batch was created.
    pub created_at: String,

    /// RFC 3339 datetime string representing the time at which processing
    /// for the Message Batch ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,

    /// RFC 3339 datetime string representing the time at which the Message
    /// Batch will expire and end processing (24 hours after creation).
    pub expires_at: String,

    /// Processing status of the Message Batch.
    pub processing_status: MessageBatchProcessingStatus,

    /// Tallies requests within the Message Batch, categorized by status.
    pub request_counts: MessageBatchRequestCounts,

    /// URL to a `.jsonl` file containing the results of the Message Batch
    /// requests. Available only once processing ends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results_url: Option<String>,

    /// Object type. Always `"message_batch"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

/// Maps to: TS MessageBatch.processing_status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageBatchProcessingStatus {
    InProgress,
    Canceling,
    Ended,
}

impl MessageBatchProcessingStatus {
    /// String value used by the TypeScript SDK and API JSON payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Canceling => "canceling",
            Self::Ended => "ended",
        }
    }
}

// --------------------------------------------------------------------------
// MessageBatchRequestCounts
// --------------------------------------------------------------------------

/// Maps to: TS MessageBatchRequestCounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatchRequestCounts {
    /// Number of requests that have been canceled.
    pub canceled: i64,

    /// Number of requests that encountered an error.
    pub errored: i64,

    /// Number of requests that have expired.
    pub expired: i64,

    /// Number of requests that are processing.
    pub processing: i64,

    /// Number of requests that have completed successfully.
    pub succeeded: i64,
}

// --------------------------------------------------------------------------
// MessageBatchResult variants
// --------------------------------------------------------------------------

/// Maps to: TS MessageBatchResult
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageBatchResult {
    /// Maps to: TS MessageBatchSucceededResult
    #[serde(rename = "succeeded")]
    Succeeded { message: Box<Message> },

    /// Maps to: TS MessageBatchErroredResult
    #[serde(rename = "errored")]
    Errored { error: ErrorResponse },

    /// Maps to: TS MessageBatchCanceledResult
    #[serde(rename = "canceled")]
    Canceled,

    /// Maps to: TS MessageBatchExpiredResult
    #[serde(rename = "expired")]
    Expired,
}

/// TS export-name compatibility alias for `MessageBatchSucceededResult`.
pub type MessageBatchSucceededResult = MessageBatchResult;
/// TS export-name compatibility alias for `MessageBatchErroredResult`.
pub type MessageBatchErroredResult = MessageBatchResult;
/// TS export-name compatibility alias for `MessageBatchCanceledResult`.
pub type MessageBatchCanceledResult = MessageBatchResult;
/// TS export-name compatibility alias for `MessageBatchExpiredResult`.
pub type MessageBatchExpiredResult = MessageBatchResult;

/// Backward-compatible Rust name for the TS `Shared.ErrorResponse` envelope.
pub type BatchErrorResponse = ErrorResponse;

// --------------------------------------------------------------------------
// MessageBatchIndividualResponse
// --------------------------------------------------------------------------

/// Maps to: TS MessageBatchIndividualResponse
///
/// A single line in the response `.jsonl` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatchIndividualResponse {
    /// Developer-provided ID created for each request in a Message Batch.
    pub custom_id: String,

    /// Processing result for this request.
    pub result: MessageBatchResult,
}

// --------------------------------------------------------------------------
// DeletedMessageBatch
// --------------------------------------------------------------------------

/// Maps to: TS DeletedMessageBatch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedMessageBatch {
    /// ID of the deleted Message Batch.
    pub id: String,

    /// Deleted object type. Always `"message_batch_deleted"`.
    #[serde(rename = "type")]
    pub type_name: String,
}

// --------------------------------------------------------------------------
// Request params
// --------------------------------------------------------------------------

/// Maps to: TS BatchCreateParams.Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    /// Developer-provided ID. Must be unique within the batch.
    pub custom_id: String,

    /// Messages API creation parameters for the individual request.
    pub params: MessageCreateParams,
}

/// TS namespace compatibility alias for `BatchCreateParams.Request`.
pub type Request = BatchRequest;

/// Maps to: TS BatchCreateParams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCreateParams {
    /// List of requests for prompt completion.
    pub requests: Vec<BatchRequest>,
}

/// Maps to: TS BatchListParams
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatchListParams {
    /// A cursor for use in pagination. `after_id` is the ID of the object to
    /// start after (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,

    /// A cursor for use in pagination. `before_id` is the ID of the object
    /// to end before (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,

    /// Number of items to return per page (defaults to 20, max 100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

// --------------------------------------------------------------------------
// Paginated response wrapper
// --------------------------------------------------------------------------

/// Maps to: TS MessageBatchesPage (Page<MessageBatch>)
///
/// Wraps a paginated list response from the batches API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBatchesPage {
    pub data: Vec<MessageBatch>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
}

// ==========================================================================
// Batches resource
// ==========================================================================

/// Maps to: TS Batches class -- resource-based accessor for the Message
/// Batches API (`/v1/messages/batches`).
///
/// Obtain an instance via [`Messages::batches`] (or create directly with a
/// client reference).
///
/// ```ignore
/// let client = Anthropic::new(ClientOptions::default())?;
/// let batch = client.messages().batches().create(&params).await?;
/// ```
pub struct Batches<'a> {
    client: &'a Anthropic,
}

impl<'a> Batches<'a> {
    /// Create a new `Batches` resource bound to the given client.
    pub fn new(client: &'a Anthropic) -> Self {
        Self { client }
    }

    /// Maps to: TS Batches.create() -- POST /v1/messages/batches
    ///
    /// Send a batch of Message creation requests. Once created, the batch
    /// begins processing immediately and can take up to 24 hours to complete.
    pub async fn create(&self, params: &BatchCreateParams) -> Result<MessageBatch, ApiError> {
        self.create_with_options(params, None).await
    }

    /// Create a message batch with per-request options.
    pub async fn create_with_options(
        &self,
        params: &BatchCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<MessageBatch, ApiError> {
        Ok(self
            .create_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.create(...).withResponse()`.
    pub async fn create_with_response(
        &self,
        params: &BatchCreateParams,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        self.create_with_response_and_options(params, None).await
    }

    /// Create a message batch returning parsed data plus raw response metadata/body.
    pub async fn create_with_response_and_options(
        &self,
        params: &BatchCreateParams,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        let body = serde_json::to_value(params)
            .map_err(|e| ApiError::Sdk(format!("failed to serialize BatchCreateParams: {e}")))?;

        self.client
            .post_with_response("/v1/messages/batches", &body, None, options)
            .await
    }

    /// Maps to: TS Batches.retrieve() -- GET /v1/messages/batches/{id}
    ///
    /// Retrieve a Message Batch by ID. This endpoint is idempotent and can be
    /// used to poll for batch completion.
    pub async fn retrieve(&self, message_batch_id: &str) -> Result<MessageBatch, ApiError> {
        self.retrieve_with_options(message_batch_id, None).await
    }

    /// Retrieve a Message Batch by ID with per-request options.
    pub async fn retrieve_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<MessageBatch, ApiError> {
        Ok(self
            .retrieve_with_response_and_options(message_batch_id, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.retrieve(...).withResponse()`.
    pub async fn retrieve_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        self.retrieve_with_response_and_options(message_batch_id, None)
            .await
    }

    /// Retrieve a Message Batch by ID returning parsed data plus raw response metadata/body.
    pub async fn retrieve_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        let message_batch_id = encode_path_param(message_batch_id)?;
        let path = format!("/v1/messages/batches/{message_batch_id}");
        self.client
            .get_with_response(&path, None, None, options)
            .await
    }

    /// Maps to: TS Batches.list() -- GET /v1/messages/batches
    ///
    /// List all Message Batches within a Workspace. Most recently created
    /// batches are returned first.
    pub async fn list(
        &self,
        params: Option<&BatchListParams>,
    ) -> Result<MessageBatchesPage, ApiError> {
        self.list_with_options(params, None).await
    }

    /// List message batches with per-request options.
    pub async fn list_with_options(
        &self,
        params: Option<&BatchListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<MessageBatchesPage, ApiError> {
        Ok(self
            .list_with_response_and_options(params, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.list(...).withResponse()`.
    pub async fn list_with_response(
        &self,
        params: Option<&BatchListParams>,
    ) -> Result<ApiResponse<MessageBatchesPage>, ApiError> {
        self.list_with_response_and_options(params, None).await
    }

    /// List message batches returning parsed data plus raw response metadata/body.
    pub async fn list_with_response_and_options(
        &self,
        params: Option<&BatchListParams>,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageBatchesPage>, ApiError> {
        let mut query: HashMap<String, Option<String>> = HashMap::new();

        if let Some(p) = params {
            if let Some(ref after_id) = p.after_id {
                query.insert("after_id".into(), Some(after_id.clone()));
            }
            if let Some(ref before_id) = p.before_id {
                query.insert("before_id".into(), Some(before_id.clone()));
            }
            if let Some(limit) = p.limit {
                query.insert("limit".into(), Some(limit.to_string()));
            }
        }

        let query_ref = if query.is_empty() { None } else { Some(&query) };

        self.client
            .get_with_response("/v1/messages/batches", query_ref, None, options)
            .await
    }

    /// Maps to: TS Batches.cancel() -- POST /v1/messages/batches/{id}/cancel
    ///
    /// Cancel a Message Batch. Batches may be canceled any time before
    /// processing ends. Once cancellation is initiated the batch enters a
    /// `canceling` state.
    pub async fn cancel(&self, message_batch_id: &str) -> Result<MessageBatch, ApiError> {
        self.cancel_with_options(message_batch_id, None).await
    }

    /// Cancel a Message Batch with per-request options.
    pub async fn cancel_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<MessageBatch, ApiError> {
        Ok(self
            .cancel_with_response_and_options(message_batch_id, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.cancel(...).withResponse()`.
    pub async fn cancel_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        self.cancel_with_response_and_options(message_batch_id, None)
            .await
    }

    /// Cancel a Message Batch returning parsed data plus raw response metadata/body.
    pub async fn cancel_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<MessageBatch>, ApiError> {
        let message_batch_id = encode_path_param(message_batch_id)?;
        let path = format!("/v1/messages/batches/{message_batch_id}/cancel");
        self.client
            .request_with_response(reqwest::Method::POST, &path, None, None, None, options)
            .await
    }

    /// Maps to: TS Batches.delete() -- DELETE /v1/messages/batches/{id}
    ///
    /// Delete a Message Batch. Batches can only be deleted once they have
    /// finished processing. To delete an in-progress batch, cancel it first.
    pub async fn delete(&self, message_batch_id: &str) -> Result<DeletedMessageBatch, ApiError> {
        self.delete_with_options(message_batch_id, None).await
    }

    /// Delete a Message Batch with per-request options.
    pub async fn delete_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<DeletedMessageBatch, ApiError> {
        Ok(self
            .delete_with_response_and_options(message_batch_id, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.delete(...).withResponse()`.
    pub async fn delete_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<DeletedMessageBatch>, ApiError> {
        self.delete_with_response_and_options(message_batch_id, None)
            .await
    }

    /// Delete a Message Batch returning parsed data plus raw response metadata/body.
    pub async fn delete_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<DeletedMessageBatch>, ApiError> {
        let message_batch_id = encode_path_param(message_batch_id)?;
        let path = format!("/v1/messages/batches/{message_batch_id}");
        self.client.delete_with_response(&path, None, options).await
    }

    /// Rust convenience helper that buffers all TS `Batches.results()` items.
    ///
    /// Retrieves the batch, then fetches and parses the results as a `Vec`
    /// of [`MessageBatchIndividualResponse`] items. Each item corresponds to
    /// one request in the original batch.
    ///
    /// Returns an error if the batch has not yet finished processing (i.e.
    /// `results_url` is `None`).
    pub async fn results_all(
        &self,
        message_batch_id: &str,
    ) -> Result<Vec<MessageBatchIndividualResponse>, ApiError> {
        self.results_all_with_options(message_batch_id, None).await
    }

    /// Fetch batch results with per-request options.
    pub async fn results_all_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<Vec<MessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_all_with_response_and_options(message_batch_id, options)
            .await?
            .data)
    }

    /// Rust equivalent of TS `client.messages.batches.results(...).withResponse()`.
    pub async fn results_all_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<Vec<MessageBatchIndividualResponse>>, ApiError> {
        self.results_all_with_response_and_options(message_batch_id, None)
            .await
    }

    /// Fetch batch results returning parsed JSONL items plus the raw binary response metadata/body.
    pub async fn results_all_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<Vec<MessageBatchIndividualResponse>>, ApiError> {
        // TS `Batches.results()` first retrieves the batch without forwarding
        // per-request options, then applies options only to the binary results
        // fetch below.
        let batch = self.retrieve(message_batch_id).await?;

        let results_url = batch.results_url.ok_or_else(|| {
            ApiError::Sdk(format!(
                "No batch `results_url`; Has it finished processing? {} - {}",
                batch.processing_status.as_str(),
                batch.id
            ))
        })?;

        let mut headers = HashMap::new();
        headers.insert("accept".to_owned(), Some("application/binary".to_owned()));

        // Fetch the JSONL file as raw bytes like TS `__binaryResponse`, then
        // decode UTF-8 and parse line-by-line.
        let response = self
            .client
            .get_binary_with_response(&results_url, None, Some(&headers), options)
            .await?;
        let data = parse_message_batch_results(&response.data)?;

        Ok(ApiResponse {
            data,
            response: response.response,
            request_id: response.request_id,
        })
    }

    /// Maps to TS `client.messages.batches.results()`.
    ///
    /// Exposes the JSONL response as a lazy [`JsonLineStream`].
    pub async fn results(
        &self,
        message_batch_id: &str,
    ) -> Result<JsonLineStream<MessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_with_response_and_options(message_batch_id, None)
            .await?
            .data)
    }

    /// Streaming batch results with per-request options.
    pub async fn results_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<JsonLineStream<MessageBatchIndividualResponse>, ApiError> {
        Ok(self
            .results_with_response_and_options(message_batch_id, options)
            .await?
            .data)
    }

    /// Streaming batch results with raw response metadata.
    pub async fn results_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<JsonLineStream<MessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response_and_options(message_batch_id, None)
            .await
    }

    /// Streaming batch results with raw response metadata and per-request options.
    pub async fn results_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<JsonLineStream<MessageBatchIndividualResponse>>, ApiError> {
        let batch = self.retrieve(message_batch_id).await?;

        let results_url = batch.results_url.ok_or_else(|| {
            ApiError::Sdk(format!(
                "No batch `results_url`; Has it finished processing? {} - {}",
                batch.processing_status.as_str(),
                batch.id
            ))
        })?;

        let mut headers = HashMap::new();
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

    /// Backward-compatible alias for [`Batches::results`].
    pub async fn results_stream(
        &self,
        message_batch_id: &str,
    ) -> Result<JsonLineStream<MessageBatchIndividualResponse>, ApiError> {
        self.results(message_batch_id).await
    }

    /// Backward-compatible alias for [`Batches::results_with_options`].
    pub async fn results_stream_with_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<JsonLineStream<MessageBatchIndividualResponse>, ApiError> {
        self.results_with_options(message_batch_id, options).await
    }

    /// Backward-compatible alias for [`Batches::results_with_response`].
    pub async fn results_stream_with_response(
        &self,
        message_batch_id: &str,
    ) -> Result<ApiResponse<JsonLineStream<MessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response(message_batch_id).await
    }

    /// Backward-compatible alias for [`Batches::results_with_response_and_options`].
    pub async fn results_stream_with_response_and_options(
        &self,
        message_batch_id: &str,
        options: Option<&RequestOptions>,
    ) -> Result<ApiResponse<JsonLineStream<MessageBatchIndividualResponse>>, ApiError> {
        self.results_with_response_and_options(message_batch_id, options)
            .await
    }
}

fn parse_message_batch_results(
    response_bytes: &[u8],
) -> Result<Vec<MessageBatchIndividualResponse>, ApiError> {
    let response_text = String::from_utf8(response_bytes.to_vec())
        .map_err(|err| ApiError::Sdk(format!("batch results were not valid UTF-8: {err}")))?;
    let mut items = Vec::new();

    for line in response_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item: MessageBatchIndividualResponse = serde_json::from_str(trimmed)
            .map_err(|e| ApiError::Sdk(format!("failed to parse batch result line: {e}")))?;
        items.push(item);
    }

    Ok(items)
}

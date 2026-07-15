// Maps to: TS core/api-promise.ts `asResponse()` / `withResponse()` support.

use std::collections::HashMap;

use serde::de::DeserializeOwned;

use crate::core::error::ApiError;

/// Buffered Rust equivalent of the raw Fetch `Response` returned by TS
/// `APIPromise.asResponse()`.
///
/// Rust/reqwest response bodies are single-consumer streams. To make raw
/// response access compatible with parsed-data access, the SDK buffers the
/// successful response body and exposes status, URL, headers, and body bytes in
/// this cloneable value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawResponse {
    /// HTTP status code.
    pub status: u16,
    /// Final response URL, after reqwest URL construction/redirect handling.
    pub url: String,
    /// Response headers. Header names are stored lower-case by reqwest.
    pub headers: HashMap<String, String>,
    /// Buffered response body bytes.
    pub body: Vec<u8>,
}

impl RawResponse {
    /// Capture response metadata without consuming the body.
    ///
    /// This is used for streaming `withResponse()`-style helpers where the
    /// response body must remain available to the event stream. The returned
    /// body is therefore intentionally empty.
    pub fn from_response_metadata(response: &reqwest::Response) -> Self {
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().unwrap_or_default().to_owned(),
                )
            })
            .collect();
        Self {
            status: response.status().as_u16(),
            url: response.url().to_string(),
            headers,
            body: Vec::new(),
        }
    }

    /// Case-insensitive response-header lookup, matching Fetch `Headers.get()`.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(name)
            .or_else(|| {
                self.headers
                    .iter()
                    .find_map(|(key, value)| key.eq_ignore_ascii_case(name).then_some(value))
            })
            .map(String::as_str)
    }

    /// TS-style alias for `header("request-id")`.
    pub fn request_id(&self) -> Option<&str> {
        self.header("request-id")
    }

    /// Decode the buffered body as UTF-8 text.
    pub fn text(&self) -> Result<String, ApiError> {
        String::from_utf8(self.body.clone()).map_err(|err| {
            ApiError::Sdk(format!(
                "response body was not valid UTF-8 for text response: {err}"
            ))
        })
    }

    /// Parse the buffered body as JSON.
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, ApiError> {
        serde_json::from_slice(&self.body)
            .map_err(|err| ApiError::Sdk(format!("failed to deserialize response: {err}")))
    }
}

/// Rust named equivalent of TS `APIPromise.withResponse()`'s result object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse<T> {
    /// Parsed response data.
    pub data: T,
    /// Buffered raw response metadata/body.
    pub response: RawResponse,
    /// Request ID from the `request-id` response header.
    pub request_id: Option<String>,
}

/// TS export-name compatibility alias for a response-with-data object.
pub type APIResponse<T> = ApiResponse<T>;

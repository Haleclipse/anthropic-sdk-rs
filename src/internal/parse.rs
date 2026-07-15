// Maps to: TS internal/parse.ts
//
//! Response parsing helpers for the Anthropic Rust SDK.
//!
//! Provides utilities for extracting metadata (like request IDs) from HTTP
//! response headers and for parsing response bodies into typed Rust structs.

use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;

use crate::core::error::ApiError;

// ---------------------------------------------------------------------------
// Request-ID extraction
// ---------------------------------------------------------------------------

/// Maps to: TS `addRequestID()` / header extraction in internal/parse.ts
///
/// Extracts the `request-id` header from the response headers, returning
/// `None` if the header is absent or its value is not valid UTF-8.
pub fn extract_request_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// Rust type alias for TS `WithRequestID<T>`.
///
/// TypeScript augments object response types with a non-enumerable
/// `_request_id` property. Rust response structs expose request IDs through
/// typed fields or [`crate::core::response::ApiResponse`], so the type-level
/// helper is an identity alias.
pub type WithRequestID<T> = T;

/// Add `_request_id` to a JSON object value, mirroring TS `addRequestID()`.
///
/// Arrays, scalars, and `null` are returned unchanged. Rust cannot attach a
/// non-enumerable property to arbitrary structs, so this helper is limited to
/// `serde_json::Value` object responses.
pub fn add_request_id(mut value: serde_json::Value, request_id: Option<&str>) -> serde_json::Value {
    if let (Some(id), Some(object)) = (request_id, value.as_object_mut()) {
        object.insert(
            "_request_id".to_owned(),
            serde_json::Value::String(id.to_owned()),
        );
    }
    value
}

/// HeaderMap-based convenience wrapper for [`add_request_id`].
pub fn add_request_id_from_headers(
    value: serde_json::Value,
    headers: &HeaderMap,
) -> serde_json::Value {
    add_request_id(value, extract_request_id(headers).as_deref())
}

/// TS-style camelCase alias for [`add_request_id_from_headers`].
#[allow(non_snake_case)]
pub fn addRequestID(value: serde_json::Value, headers: &HeaderMap) -> serde_json::Value {
    add_request_id_from_headers(value, headers)
}

// ---------------------------------------------------------------------------
// Response body parsing
// ---------------------------------------------------------------------------

/// Maps to: TS `defaultParseResponse()` in internal/parse.ts.
///
/// Parses JSON responses as JSON, non-JSON responses as raw text, and 204/empty
/// JSON bodies as `null`, returning an `ApiError::Sdk` if deserialization into
/// `T` fails.
pub async fn parse_json_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, ApiError> {
    let status = response.status();
    let headers = response.headers().clone();
    let request_id = extract_request_id(&headers);

    let bytes = response.bytes().await.map_err(|e| ApiError::Connection {
        message: format!("failed to read response body: {e}"),
        cause: Some(Box::new(e)),
    })?;

    // 204 No Content -- nothing to parse.
    if status.as_u16() == 204 {
        // Attempt zero-value deserialization (works for Option<T>, (), etc.).
        return serde_json::from_slice::<T>(b"null").map_err(|e| {
            ApiError::Sdk(format!(
                "204 No Content but could not produce a zero-value for type: {e}"
            ))
        });
    }

    // Check Content-Type -- only parse JSON if the response says it's JSON.
    let is_json = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            let media = ct.split(';').next().unwrap_or("").trim();
            media.contains("application/json") || media.ends_with("+json")
        })
        .unwrap_or(false);

    if is_json {
        let content_length_zero = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|value| value == "0");
        if bytes.is_empty() || content_length_zero {
            // Empty body with JSON content-type.
            return serde_json::from_slice::<T>(b"null").map_err(|e| {
                ApiError::Sdk(format!(
                    "empty JSON body but could not produce a zero-value for type: {e}"
                ))
            });
        }

        let value = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| {
            ApiError::Sdk(format!(
                "failed to parse JSON response (request_id={request_id:?}): {e}"
            ))
        })?;

        let value = add_request_id(value, request_id.as_deref());

        serde_json::from_value::<T>(value).map_err(|e| {
            ApiError::Sdk(format!(
                "failed to deserialize JSON response (request_id={request_id:?}): {e}"
            ))
        })
    } else {
        let text = String::from_utf8(bytes.to_vec()).map_err(|e| {
            ApiError::Sdk(format!(
                "response body was not valid UTF-8 for non-JSON response: {e}"
            ))
        })?;
        serde_json::from_value(serde_json::Value::String(text)).map_err(|e| {
            ApiError::Sdk(format!(
                "failed to deserialize text response (request_id={request_id:?}): {e}"
            ))
        })
    }
}

/// Rust alias for [`parse_json_response`] using TS `defaultParseResponse`
/// terminology.
pub async fn default_parse_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<WithRequestID<T>, ApiError> {
    parse_json_response(response).await
}

/// TS-style camelCase alias for [`default_parse_response`].
#[allow(non_snake_case)]
pub async fn defaultParseResponse<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<WithRequestID<T>, ApiError> {
    default_parse_response(response).await
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    #[test]
    fn extract_request_id_present() {
        let mut headers = HeaderMap::new();
        headers.insert("request-id", HeaderValue::from_static("req_abc123"));
        assert_eq!(extract_request_id(&headers), Some("req_abc123".to_string()));
    }

    #[test]
    fn extract_request_id_missing() {
        let headers = HeaderMap::new();
        assert_eq!(extract_request_id(&headers), None);
    }

    #[test]
    fn extract_request_id_case_insensitive() {
        // reqwest HeaderMap is case-insensitive by design.
        let mut headers = HeaderMap::new();
        headers.insert("Request-Id", HeaderValue::from_static("req_xyz"));
        assert_eq!(extract_request_id(&headers), Some("req_xyz".to_string()));
    }

    #[test]
    fn add_request_id_matches_ts_object_only_behavior() {
        let mut headers = HeaderMap::new();
        headers.insert("request-id", HeaderValue::from_static("req_add"));

        assert_eq!(
            addRequestID(serde_json::json!({"id": "bar"}), &headers),
            serde_json::json!({"id": "bar", "_request_id": "req_add"})
        );
        assert_eq!(
            add_request_id(serde_json::json!([{"id": "bar"}]), Some("req_add")),
            serde_json::json!([{"id": "bar"}])
        );
        assert_eq!(
            add_request_id(serde_json::json!("text"), Some("req_add")),
            serde_json::json!("text")
        );
        assert_eq!(
            add_request_id(serde_json::Value::Null, Some("req_add")),
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn default_parse_response_alias_matches_parse_json_response() {
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .header("request-id", "req_default_parse")
            .body(r#"{"id":"bar"}"#)
            .unwrap()
            .into();

        let value: serde_json::Value = defaultParseResponse(response).await.unwrap();
        assert_eq!(
            value,
            serde_json::json!({"id": "bar", "_request_id": "req_default_parse"})
        );
    }
}

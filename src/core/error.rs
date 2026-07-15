// Maps to: TS core/error.ts

use std::collections::HashMap;

pub(crate) fn json_value_is_js_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => true,
    }
}

/// API error types matching the Anthropic API error hierarchy.
/// Maps status codes to specific error variants, with connection/timeout/abort
/// variants for non-HTTP errors.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    BadRequest {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    Authentication {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    PermissionDenied {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    NotFound {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    Conflict {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    UnprocessableEntity {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    RateLimitError {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    InternalServerError {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("{message}")]
    Connection {
        message: String,
        #[source]
        cause: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("{message}")]
    ConnectionTimeout { message: String },

    #[error("{message}")]
    UserAbort { message: String },

    #[error("{message}")]
    Other {
        status: u16,
        message: String,
        headers: HashMap<String, String>,
        request_id: Option<String>,
        body: Option<Box<serde_json::Value>>,
    },

    #[error("SDK error: {0}")]
    Sdk(String),
}

/// TS export-name compatibility alias for `AnthropicError`.
///
/// Rust models the TS error class hierarchy as [`ApiError`] enum variants; these
/// aliases make root/core imports line up with TS `src/core/error.ts` names while
/// keeping variant matching idiomatic.
pub type AnthropicError = ApiError;

/// TS export-name compatibility alias for `APIError`.
pub type APIError = ApiError;
/// TS export-name compatibility alias for `APIConnectionError`.
pub type APIConnectionError = ApiError;
/// TS export-name compatibility alias for `APIConnectionTimeoutError`.
pub type APIConnectionTimeoutError = ApiError;
/// TS export-name compatibility alias for `APIUserAbortError`.
pub type APIUserAbortError = ApiError;
/// TS export-name compatibility alias for `BadRequestError`.
pub type BadRequestError = ApiError;
/// TS export-name compatibility alias for `AuthenticationError`.
pub type AuthenticationError = ApiError;
/// TS export-name compatibility alias for `PermissionDeniedError`.
pub type PermissionDeniedError = ApiError;
/// TS export-name compatibility alias for `NotFoundError`.
pub type NotFoundError = ApiError;
/// TS export-name compatibility alias for `ConflictError`.
pub type ConflictError = ApiError;
/// TS export-name compatibility alias for `UnprocessableEntityError`.
pub type UnprocessableEntityError = ApiError;
/// TS export-name compatibility alias for `RateLimitError`.
pub type RateLimitError = ApiError;
/// TS export-name compatibility alias for `InternalServerError`.
pub type InternalServerError = ApiError;

impl ApiError {
    /// Maps to: TS `APIError.generate()` — dispatches on status code to the
    /// correct variant. No status/headers → Connection error.
    pub fn generate(
        status: Option<u16>,
        error_body: Option<serde_json::Value>,
        message: Option<String>,
        headers: Option<HashMap<String, String>>,
    ) -> Self {
        let Some(status) = status else {
            return ApiError::Connection {
                message: message.unwrap_or_else(|| "Connection error.".to_owned()),
                cause: None,
            };
        };
        if status == 0 {
            return ApiError::Connection {
                message: message.unwrap_or_else(|| "Connection error.".to_owned()),
                cause: None,
            };
        }

        let Some(headers) = headers else {
            return ApiError::Connection {
                message: message.unwrap_or_else(|| "Connection error.".to_owned()),
                cause: None,
            };
        };
        let request_id = get_header_case_insensitive(&headers, "request-id").cloned();
        let msg = Self::make_message(Some(status), error_body.as_ref(), message.as_deref());

        match status {
            400 => ApiError::BadRequest {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            401 => ApiError::Authentication {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            403 => ApiError::PermissionDenied {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            404 => ApiError::NotFound {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            409 => ApiError::Conflict {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            422 => ApiError::UnprocessableEntity {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            429 => ApiError::RateLimitError {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            s if s >= 500 => ApiError::InternalServerError {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
            _ => ApiError::Other {
                status,
                message: msg,
                headers,
                request_id,
                body: error_body.map(Box::new),
            },
        }
    }

    /// Maps to: TS `APIError.makeMessage()` — constructs a human-readable
    /// message from status + error body + fallback message.
    fn make_message(
        status: Option<u16>,
        error: Option<&serde_json::Value>,
        message: Option<&str>,
    ) -> String {
        let fallback_message = || {
            message
                .filter(|message| !message.is_empty())
                .map(String::from)
        };
        let msg = match error {
            Some(error) => {
                if let Some(message_value) = error.get("message") {
                    if json_value_is_js_truthy(message_value) {
                        Some(match message_value {
                            serde_json::Value::String(message) => message.clone(),
                            other => other.to_string(),
                        })
                    } else if json_value_is_js_truthy(error) {
                        Some(error.to_string())
                    } else {
                        fallback_message()
                    }
                } else if json_value_is_js_truthy(error) {
                    Some(error.to_string())
                } else {
                    fallback_message()
                }
            }
            None => fallback_message(),
        };

        match (status, msg) {
            (Some(s), Some(m)) => format!("{s} {m}"),
            (Some(s), None) => format!("{s} status code (no body)"),
            (None, Some(m)) => m,
            (None, None) => "(no status code or body)".to_string(),
        }
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            ApiError::BadRequest { request_id, .. }
            | ApiError::Authentication { request_id, .. }
            | ApiError::PermissionDenied { request_id, .. }
            | ApiError::NotFound { request_id, .. }
            | ApiError::Conflict { request_id, .. }
            | ApiError::UnprocessableEntity { request_id, .. }
            | ApiError::RateLimitError { request_id, .. }
            | ApiError::InternalServerError { request_id, .. }
            | ApiError::Other { request_id, .. } => request_id.as_deref(),
            _ => None,
        }
    }

    /// TS-style alias for the `requestID` property on `APIError`.
    #[allow(non_snake_case)]
    pub fn requestID(&self) -> Option<&str> {
        self.request_id()
    }

    pub fn headers(&self) -> Option<&HashMap<String, String>> {
        match self {
            ApiError::BadRequest { headers, .. }
            | ApiError::Authentication { headers, .. }
            | ApiError::PermissionDenied { headers, .. }
            | ApiError::NotFound { headers, .. }
            | ApiError::Conflict { headers, .. }
            | ApiError::UnprocessableEntity { headers, .. }
            | ApiError::RateLimitError { headers, .. }
            | ApiError::InternalServerError { headers, .. }
            | ApiError::Other { headers, .. } => Some(headers),
            _ => None,
        }
    }

    /// Return an error response header by name using TS `Headers.get()`-style
    /// case-insensitive lookup.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers()
            .and_then(|headers| get_header_case_insensitive(headers, name))
            .map(String::as_str)
    }

    pub fn body(&self) -> Option<&serde_json::Value> {
        match self {
            ApiError::BadRequest { body, .. }
            | ApiError::Authentication { body, .. }
            | ApiError::PermissionDenied { body, .. }
            | ApiError::NotFound { body, .. }
            | ApiError::Conflict { body, .. }
            | ApiError::UnprocessableEntity { body, .. }
            | ApiError::RateLimitError { body, .. }
            | ApiError::InternalServerError { body, .. }
            | ApiError::Other { body, .. } => body.as_deref(),
            _ => None,
        }
    }

    /// TS names the parsed error body field `error`; keep `body()` for Rust
    /// callers while also exposing the TS-aligned accessor name.
    pub fn error(&self) -> Option<&serde_json::Value> {
        self.body()
    }

    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::BadRequest { status, .. }
            | ApiError::Authentication { status, .. }
            | ApiError::PermissionDenied { status, .. }
            | ApiError::NotFound { status, .. }
            | ApiError::Conflict { status, .. }
            | ApiError::UnprocessableEntity { status, .. }
            | ApiError::RateLimitError { status, .. }
            | ApiError::InternalServerError { status, .. }
            | ApiError::Other { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Maps to: TS `shouldRetry()` — 408, 409, 429, 5xx are retryable.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.status(),
            Some(408) | Some(409) | Some(429) | Some(500..)
        )
    }
}

fn get_header_case_insensitive<'a>(
    headers: &'a HashMap<String, String>,
    name: &str,
) -> Option<&'a String> {
    headers.get(name).or_else(|| {
        headers
            .iter()
            .find_map(|(k, v)| k.eq_ignore_ascii_case(name).then_some(v))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_connection_error_defaults_match_ts() {
        let err = ApiError::generate(None, None, None, None);
        assert!(matches!(err, ApiError::Connection { .. }));
        assert_eq!(err.to_string(), "Connection error.");
        assert_eq!(err.status(), None);
        assert_eq!(err.request_id(), None);
        assert!(err.headers().is_none());
        assert!(err.body().is_none());
    }

    #[test]
    fn generate_with_status_but_no_headers_is_connection_error_like_ts() {
        let err = ApiError::generate(
            Some(400),
            Some(serde_json::json!({"message": "bad"})),
            None,
            None,
        );
        assert!(matches!(err, ApiError::Connection { .. }));
        assert_eq!(err.to_string(), "Connection error.");
        assert_eq!(err.status(), None);
        assert!(err.headers().is_none());
        assert!(err.error().is_none());
    }

    #[test]
    fn generate_with_zero_status_is_connection_error_like_ts() {
        let err = ApiError::generate(
            Some(0),
            None,
            Some("network-ish".to_owned()),
            Some(HashMap::new()),
        );
        assert!(matches!(err, ApiError::Connection { .. }));
        assert_eq!(err.to_string(), "network-ish");
        assert_eq!(err.status(), None);
        assert!(err.headers().is_none());
    }

    #[test]
    fn generate_message_formatting_matches_ts_truthiness_rules() {
        let headers = || Some(HashMap::new());

        let err = ApiError::generate(Some(400), None, Some(String::new()), headers());
        assert_eq!(err.to_string(), "400 status code (no body)");

        let err = ApiError::generate(
            Some(400),
            Some(serde_json::json!({"message": "bad"})),
            None,
            headers(),
        );
        assert_eq!(err.to_string(), "400 bad");

        let err = ApiError::generate(
            Some(400),
            Some(serde_json::json!({"message": ""})),
            None,
            headers(),
        );
        assert_eq!(err.to_string(), "400 {\"message\":\"\"}");

        let err = ApiError::generate(
            Some(400),
            Some(serde_json::json!({"message": false, "type": "error"})),
            None,
            headers(),
        );
        assert_eq!(
            err.to_string(),
            "400 {\"message\":false,\"type\":\"error\"}"
        );

        let err = ApiError::generate(
            Some(400),
            Some(serde_json::json!(false)),
            Some("false".to_owned()),
            headers(),
        );
        assert_eq!(err.to_string(), "400 false");
    }

    #[test]
    fn generate_status_dispatch_matches_ts_api_error_generate() {
        type ErrorVariantMatcher = fn(&ApiError) -> bool;
        let cases: Vec<(u16, ErrorVariantMatcher)> = vec![
            (400, |err| matches!(err, ApiError::BadRequest { .. })),
            (401, |err| matches!(err, ApiError::Authentication { .. })),
            (403, |err| matches!(err, ApiError::PermissionDenied { .. })),
            (404, |err| matches!(err, ApiError::NotFound { .. })),
            (409, |err| matches!(err, ApiError::Conflict { .. })),
            (422, |err| {
                matches!(err, ApiError::UnprocessableEntity { .. })
            }),
            (429, |err| matches!(err, ApiError::RateLimitError { .. })),
            (500, |err| {
                matches!(err, ApiError::InternalServerError { .. })
            }),
            (599, |err| {
                matches!(err, ApiError::InternalServerError { .. })
            }),
            (418, |err| matches!(err, ApiError::Other { .. })),
        ];

        for (status, matches_expected_variant) in cases {
            let err = ApiError::generate(
                Some(status),
                Some(serde_json::json!({"message": format!("status {status}")})),
                None,
                Some(HashMap::from([(
                    "request-id".to_owned(),
                    format!("req_{status}"),
                )])),
            );
            assert!(
                matches_expected_variant(&err),
                "status {status} produced unexpected variant: {err:?}"
            );
            assert_eq!(err.status(), Some(status));
            assert_eq!(err.request_id(), Some(format!("req_{status}").as_str()));
            assert_eq!(err.to_string(), format!("{status} status {status}"));
        }
    }

    #[test]
    fn generated_status_error_exposes_headers_body_and_request_id() {
        let mut headers = HashMap::new();
        headers.insert("Request-ID".to_owned(), "req_123".to_owned());
        headers.insert("x-test".to_owned(), "ok".to_owned());
        let body = serde_json::json!({"error": {"message": "bad"}});

        let err = ApiError::generate(Some(400), Some(body.clone()), None, Some(headers));
        assert!(matches!(err, ApiError::BadRequest { .. }));
        assert_eq!(err.status(), Some(400));
        assert_eq!(err.request_id(), Some("req_123"));
        assert_eq!(err.requestID(), Some("req_123"));
        assert_eq!(
            err.headers().unwrap().get("x-test").map(String::as_str),
            Some("ok")
        );
        assert_eq!(err.header("x-test"), Some("ok"));
        assert_eq!(err.header("X-Test"), Some("ok"));
        assert_eq!(err.header("request-id"), Some("req_123"));
        assert_eq!(err.header("missing"), None);
        assert_eq!(err.body(), Some(&body));
        assert_eq!(err.error(), Some(&body));
    }
}

// Maps to: TS internal/request-options.ts
//
//! Per-request option overrides for the Anthropic Rust SDK.
//!
//! These allow callers to customise method/path, body, headers, query params,
//! timeout, retry count, cancellation, and generated default base URL on a
//! per-request basis, overriding the client-level defaults where TS does.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::future::BoxFuture;
use serde_json::Value;
use tokio::sync::watch;

use crate::core::error::ApiError;
use crate::internal::headers::HeadersLike;

// ---------------------------------------------------------------------------
// Request encoding helpers
// ---------------------------------------------------------------------------

/// Maps to TS `EncodedContent` in `internal/request-options.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedContent {
    /// Headers produced by the body encoder, e.g. `content-type`.
    pub body_headers: HeadersLike,
    /// Encoded request body. Rust represents TS `BodyInit` as bytes.
    pub body: Bytes,
}

/// Rust request encoder input corresponding to TS `{ headers, body }`.
#[derive(Debug, Clone, Copy)]
pub struct RequestEncoderInput<'a> {
    pub headers: &'a HeadersLike,
    pub body: &'a Value,
}

/// Maps to TS `RequestEncoder`.
pub type RequestEncoder =
    for<'a> fn(RequestEncoderInput<'a>) -> Result<EncodedContent, serde_json::Error>;

/// Maps to TS `FallbackEncoder`.
///
/// Like the TypeScript helper, this encoder ignores the incoming headers,
/// emits `content-type: application/json`, and serializes the body with JSON
/// stringification semantics.
pub fn fallback_encoder(
    request: RequestEncoderInput<'_>,
) -> Result<EncodedContent, serde_json::Error> {
    let _ = request.headers;
    let mut body_headers = HeadersLike::new();
    body_headers.insert(
        "content-type".to_owned(),
        Some("application/json".to_owned()),
    );

    Ok(EncodedContent {
        body_headers,
        body: Bytes::from(serde_json::to_vec(request.body)?),
    })
}

/// TS-style alias for [`fallback_encoder`].
#[allow(non_snake_case)]
pub fn FallbackEncoder(
    request: RequestEncoderInput<'_>,
) -> Result<EncodedContent, serde_json::Error> {
    fallback_encoder(request)
}

// ---------------------------------------------------------------------------
// Go-native request body and middleware helpers
// ---------------------------------------------------------------------------

/// Raw request body override for Go-style `option.WithRequestBody` parity.
///
/// This is intentionally Rust-native rather than DOM `BodyInit`: callers pass
/// bytes plus an optional content type, and the core client still applies SDK
/// auth/retry/timeout/abort/header behavior around the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRequestBody {
    /// Optional `Content-Type` header to install when this body is used.
    pub content_type: Option<String>,
    /// Serialized request body bytes.
    pub body: Bytes,
}

impl RawRequestBody {
    /// Create a raw body override with an optional content type.
    pub fn new(content_type: Option<impl Into<String>>, body: impl Into<Bytes>) -> Self {
        Self {
            content_type: content_type.map(Into::into),
            body: body.into(),
        }
    }

    /// Create a raw body override with a required content type.
    pub fn with_content_type(content_type: impl Into<String>, body: impl Into<Bytes>) -> Self {
        Self::new(Some(content_type), body)
    }
}

/// JSON body patch operation for Go-style `option.WithJSONSet` /
/// `option.WithJSONDel` parity.
///
/// Paths use the common dot-separated shape from Go's `sjson` options for the
/// SDK use-cases covered here (for example `metadata.trace_id` or `items.0`).
#[derive(Debug, Clone, PartialEq)]
pub enum JsonBodyPatch {
    /// Set a JSON value at `path`, creating intermediate objects/arrays.
    Set { path: String, value: Value },
    /// Delete a JSON value at `path` if it exists.
    Delete { path: String },
}

impl JsonBodyPatch {
    pub fn set(path: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::Set {
            path: path.into(),
            value: value.into(),
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self::Delete { path: path.into() }
    }
}

/// Rust-native request middleware hook inspired by Go SDK
/// `option.WithMiddleware`.
///
/// Middleware may mutate the prepared [`reqwest::Request`] before it is sent
/// and observe the response or final connection error. It is deliberately not a
/// browser/fetch shim; use a custom `reqwest::Client` when transport-level
/// behavior is required.
pub trait HttpMiddleware: Send + Sync {
    /// Mutate or validate the prepared request before it is sent.
    fn before_request<'a>(
        &'a self,
        _request: &'a mut reqwest::Request,
    ) -> BoxFuture<'a, Result<(), ApiError>> {
        Box::pin(async { Ok(()) })
    }

    /// Observe a response after it is received and before SDK parsing.
    fn on_response<'a>(
        &'a self,
        _response: &'a reqwest::Response,
    ) -> BoxFuture<'a, Result<(), ApiError>> {
        Box::pin(async { Ok(()) })
    }

    /// Observe the final send error before it is returned to the retry loop.
    fn on_error<'a>(&'a self, _error: &'a ApiError) -> BoxFuture<'a, Result<(), ApiError>> {
        Box::pin(async { Ok(()) })
    }
}

// ---------------------------------------------------------------------------
// AbortSignal
// ---------------------------------------------------------------------------

/// Maps to: TS `AbortSignal` in internal/request-options.ts
///
/// Cooperative cancellation handle that can be cloned and shared across
/// tasks.  Wraps a `tokio::sync::watch` channel so that callers can signal
/// cancellation from any thread.
#[derive(Debug, Clone)]
pub struct AbortSignal {
    rx: watch::Receiver<bool>,
    _tx: Arc<watch::Sender<bool>>,
}

impl AbortSignal {
    /// Creates a new signal in the non-aborted state.
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            rx,
            _tx: Arc::new(tx),
        }
    }

    /// Creates a signal together with the sender half that can trigger
    /// cancellation.  This is the typical constructor: keep the
    /// `AbortHandle` and pass the `AbortSignal` into `RequestOptions`.
    pub fn pair() -> (AbortHandle, Self) {
        let (tx, rx) = watch::channel(false);
        let tx = Arc::new(tx);
        let handle = AbortHandle {
            tx: Arc::clone(&tx),
        };
        let signal = Self { rx, _tx: tx };
        (handle, signal)
    }

    /// Returns `true` if the signal has been aborted.
    pub fn is_aborted(&self) -> bool {
        *self.rx.borrow()
    }

    /// Clone of the underlying watch receiver for cooperative abort polls
    /// outside the SDK request path (e.g. retry-loop `abort_rx`).
    pub fn watch_receiver(&self) -> watch::Receiver<bool> {
        self.rx.clone()
    }

    /// Waits until the signal is aborted.  Returns immediately if already
    /// aborted.
    pub async fn aborted(&mut self) {
        // If already aborted, return immediately.
        if *self.rx.borrow() {
            return;
        }
        // Wait for the value to change to `true`.
        // The loop handles spurious wakeups.
        loop {
            if self.rx.changed().await.is_err() {
                // Sender dropped -- treat as never-abort (caller will
                // time out or complete normally).
                futures::future::pending::<()>().await;
            }
            if *self.rx.borrow() {
                return;
            }
        }
    }
}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AbortHandle
// ---------------------------------------------------------------------------

/// The sender half of an [`AbortSignal`].  Call [`abort`](AbortHandle::abort)
/// to cancel the associated request.
#[derive(Debug, Clone)]
pub struct AbortHandle {
    tx: Arc<watch::Sender<bool>>,
}

impl AbortHandle {
    /// Signal cancellation.  All clones of the associated [`AbortSignal`]
    /// will observe the abort.
    pub fn abort(&self) {
        let _ = self.tx.send(true);
    }
}

// ---------------------------------------------------------------------------
// RequestOptions
// ---------------------------------------------------------------------------

/// Maps to: TS `RequestOptions` in internal/request-options.ts
///
/// Per-request overrides that can be passed alongside the request body.
/// All fields are optional -- when `None`, the client-level default is used.
#[derive(Clone, Default)]
pub struct RequestOptions {
    /// HTTP method override.
    ///
    /// Maps to: TS `RequestOptions.method`. Most resource helpers set this
    /// automatically; this escape hatch is useful for tests and custom calls.
    pub method: Option<reqwest::Method>,

    /// URL path override.
    ///
    /// Maps to: TS `RequestOptions.path`. Resource helpers normally supply the
    /// generated path; callers may override it for testing or custom endpoints.
    pub path: Option<String>,

    /// Extra HTTP headers to merge with the client-level defaults.
    ///
    /// Maps to: TS `RequestOptions.headers`
    pub headers: Option<HeadersLike>,

    /// Extra query parameters to merge with resource-level query params.
    ///
    /// Maps to: TS `RequestOptions.query`. A value of `None` removes a
    /// parameter that was set in an earlier layer.
    pub query: Option<HashMap<String, Option<String>>>,

    /// Request body override for JSON/core requests.
    ///
    /// Maps to TS `RequestOptions.body`. Stainless resource helpers in TS spread
    /// `...options` after their generated `body`, so this override can replace
    /// the generated JSON body for custom calls/tests. Multipart helper methods
    /// keep their explicit multipart body construction.
    pub body: Option<serde_json::Value>,

    /// Raw serialized request body override.
    ///
    /// This is the Rust equivalent of Go SDK `option.WithRequestBody`: it
    /// replaces the generated JSON or multipart body while preserving SDK
    /// auth/retry/timeout/abort/header behavior.
    pub raw_body: Option<RawRequestBody>,

    /// JSON body patches applied after generated/request-option JSON body
    /// selection and before serialization.
    ///
    /// This covers the Go SDK's `option.WithJSONSet` / `WithJSONDel` escape
    /// hatch without modelling JavaScript `RequestInit` fields.
    pub json_body_patches: Vec<JsonBodyPatch>,

    /// Request-level timeout override.
    ///
    /// Maps to: TS `RequestOptions.timeout` (milliseconds in TS, `Duration`
    /// in Rust for type safety).
    pub timeout: Option<Duration>,

    /// Request-level HTTP client override.
    ///
    /// This is the Rust equivalent of TS `RequestOptions.fetchOptions` / custom
    /// fetch behavior for a single call. SDK timeout/abort/retry/header logic is
    /// still applied around this client.
    pub http_client: Option<reqwest::Client>,

    /// Per-request middleware appended after client-level middleware.
    ///
    /// Inspired by Go SDK `option.WithMiddleware`; useful for Rust-native
    /// request instrumentation and header/body inspection.
    pub middlewares: Vec<Arc<dyn HttpMiddleware>>,

    /// Maximum number of retries for transient failures (network errors,
    /// 5xx, 429, etc.).
    ///
    /// Maps to: TS `RequestOptions.maxRetries`
    pub max_retries: Option<u32>,

    /// A cancellation signal that can be used to abort the in-flight request.
    ///
    /// Maps to: TS `RequestOptions.signal` (`AbortSignal`).
    pub signal: Option<AbortSignal>,

    /// A caller-supplied idempotency key.
    ///
    /// Maps to: TS `RequestOptions.idempotencyKey`. The Anthropic API client
    /// does not currently configure an idempotency header, matching the TS
    /// SDK's no-op behavior for this option unless a subclass supplies one.
    pub idempotency_key: Option<String>,

    /// Override the generated default base URL for this request when the
    /// client itself is still using the SDK default base URL.
    ///
    /// Maps to: TS `RequestOptions.defaultBaseURL`.
    pub default_base_url: Option<String>,
}

impl fmt::Debug for RequestOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestOptions")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("headers", &self.headers)
            .field("query", &self.query)
            .field("body", &self.body)
            .field("raw_body", &self.raw_body)
            .field("json_body_patches", &self.json_body_patches)
            .field("timeout", &self.timeout)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "Some(<reqwest::Client>)"),
            )
            .field("middlewares", &self.middlewares.len())
            .field("max_retries", &self.max_retries)
            .field(
                "signal",
                &self.signal.as_ref().map(|_| "Some(<AbortSignal>)"),
            )
            .field("idempotency_key", &self.idempotency_key)
            .field("default_base_url", &self.default_base_url)
            .finish()
    }
}

impl RequestOptions {
    /// Set a raw request body override with a content type.
    pub fn with_raw_body(
        mut self,
        content_type: impl Into<String>,
        body: impl Into<Bytes>,
    ) -> Self {
        self.raw_body = Some(RawRequestBody::with_content_type(content_type, body));
        self
    }

    /// Go-style alias for [`RequestOptions::with_raw_body`].
    pub fn with_request_body(
        self,
        content_type: impl Into<String>,
        body: impl Into<Bytes>,
    ) -> Self {
        self.with_raw_body(content_type, body)
    }

    /// Add a JSON set patch using dot-separated path syntax.
    pub fn with_json_set(mut self, path: impl Into<String>, value: impl Into<Value>) -> Self {
        self.json_body_patches.push(JsonBodyPatch::set(path, value));
        self
    }

    /// Add a JSON delete patch using dot-separated path syntax.
    pub fn with_json_del(mut self, path: impl Into<String>) -> Self {
        self.json_body_patches.push(JsonBodyPatch::delete(path));
        self
    }

    /// Alias spelling for callers that prefer `delete` over `del`.
    pub fn with_json_delete(self, path: impl Into<String>) -> Self {
        self.with_json_del(path)
    }

    /// Append a per-request middleware value.
    pub fn with_middleware<M>(mut self, middleware: M) -> Self
    where
        M: HttpMiddleware + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// Append a shared per-request middleware value.
    pub fn with_middleware_arc(mut self, middleware: Arc<dyn HttpMiddleware>) -> Self {
        self.middlewares.push(middleware);
        self
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fallback_encoder_matches_ts_json_fallback_encoder() {
        let mut headers = HeadersLike::new();
        headers.insert("x-existing".to_owned(), Some("kept-out".to_owned()));
        let body = json!({"message": "hello", "n": 1});

        let encoded = fallback_encoder(RequestEncoderInput {
            headers: &headers,
            body: &body,
        })
        .unwrap();
        assert_eq!(
            encoded
                .body_headers
                .get("content-type")
                .and_then(|v| v.as_deref()),
            Some("application/json")
        );
        assert!(!encoded.body_headers.contains_key("x-existing"));
        assert_eq!(&encoded.body[..], br#"{"message":"hello","n":1}"#);

        let alias = FallbackEncoder(RequestEncoderInput {
            headers: &headers,
            body: &json!("string body"),
        })
        .unwrap();
        assert_eq!(&alias.body[..], br#""string body""#);
    }

    #[test]
    fn request_options_default_is_all_none() {
        let opts = RequestOptions::default();
        assert!(opts.method.is_none());
        assert!(opts.path.is_none());
        assert!(opts.headers.is_none());
        assert!(opts.query.is_none());
        assert!(opts.body.is_none());
        assert!(opts.raw_body.is_none());
        assert!(opts.json_body_patches.is_empty());
        assert!(opts.timeout.is_none());
        assert!(opts.http_client.is_none());
        assert!(opts.middlewares.is_empty());
        assert!(opts.max_retries.is_none());
        assert!(opts.signal.is_none());
        assert!(opts.idempotency_key.is_none());
        assert!(opts.default_base_url.is_none());
    }

    #[test]
    fn request_options_with_values() {
        let mut headers = HeadersLike::new();
        headers.insert("X-Custom".into(), Some("value".into()));

        let (_, signal) = AbortSignal::pair();

        let mut query = HashMap::new();
        query.insert("limit".into(), Some("10".into()));

        let opts = RequestOptions {
            method: Some(reqwest::Method::PATCH),
            path: Some("/v1/custom".to_owned()),
            headers: Some(headers),
            query: Some(query),
            body: Some(json!({"override": true})),
            raw_body: Some(RawRequestBody::with_content_type(
                "application/custom",
                Bytes::from_static(b"raw"),
            )),
            json_body_patches: vec![JsonBodyPatch::set("metadata.trace_id", "trace_123")],
            timeout: Some(Duration::from_secs(30)),
            http_client: Some(reqwest::Client::new()),
            middlewares: Vec::new(),
            max_retries: Some(5),
            signal: Some(signal),
            idempotency_key: Some("idem_123".to_owned()),
            default_base_url: Some("https://example.com/default".to_owned()),
        };

        assert_eq!(opts.method, Some(reqwest::Method::PATCH));
        assert_eq!(opts.path.as_deref(), Some("/v1/custom"));
        assert!(opts.headers.is_some());
        assert!(opts.query.is_some());
        assert_eq!(opts.body, Some(json!({"override": true})));
        assert_eq!(
            opts.raw_body
                .as_ref()
                .map(|body| body.content_type.as_deref()),
            Some(Some("application/custom"))
        );
        assert_eq!(opts.json_body_patches.len(), 1);
        assert_eq!(opts.timeout, Some(Duration::from_secs(30)));
        assert!(opts.http_client.is_some());
        assert!(opts.middlewares.is_empty());
        assert_eq!(opts.max_retries, Some(5));
        assert!(opts.signal.is_some());
        assert_eq!(opts.idempotency_key.as_deref(), Some("idem_123"));
        assert_eq!(
            opts.default_base_url.as_deref(),
            Some("https://example.com/default")
        );
    }

    #[test]
    fn abort_signal_starts_not_aborted() {
        let (_, signal) = AbortSignal::pair();
        assert!(!signal.is_aborted());
    }

    #[test]
    fn abort_handle_signals_abort() {
        let (handle, signal) = AbortSignal::pair();
        handle.abort();
        assert!(signal.is_aborted());
    }

    #[tokio::test]
    async fn abort_signal_waits() {
        let (handle, mut signal) = AbortSignal::pair();

        // Spawn a task that aborts after a short delay.
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            handle.abort();
        });

        signal.aborted().await;
        assert!(signal.is_aborted());
    }
}

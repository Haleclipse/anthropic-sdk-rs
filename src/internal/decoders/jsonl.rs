// Maps to: TS internal/decoders/jsonl.ts
//
//! Newline-delimited JSON decoder.
//!
//! This is a small Rust wrapper around the core [`JsonLineStream`] so internal
//! callers can use the same `JSONLDecoder::fromResponse(...)` shape as the TS
//! SDK while preserving idiomatic Rust stream semantics.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream as FuturesStream;
use pin_project_lite::pin_project;

use crate::core::error::ApiError;
use crate::core::streaming::JsonLineStream;

pin_project! {
    /// Stream decoder for newline-separated JSON values.
    ///
    /// Maps to TS `JSONLDecoder<T>` from `src/internal/decoders/jsonl.ts`.
    pub struct JSONLDecoder<T> {
        #[pin]
        inner: JsonLineStream<T>,
    }
}

impl<T> JSONLDecoder<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Create a JSONL decoder from an arbitrary byte stream.
    pub fn new<S, E>(stream: S) -> Self
    where
        S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            inner: JsonLineStream::new(stream),
        }
    }

    /// Create a JSONL decoder from a `reqwest::Response` body.
    ///
    /// Maps to TS `JSONLDecoder.fromResponse(response, controller)`. Rust drops
    /// the stream to cancel instead of carrying an explicit `AbortController`.
    pub fn from_response(response: reqwest::Response) -> Self {
        Self::new(response.bytes_stream())
    }

    /// TS-style camelCase alias for [`JSONLDecoder::from_response`].
    #[allow(non_snake_case)]
    pub fn fromResponse(response: reqwest::Response) -> Self {
        Self::from_response(response)
    }

    /// Expose the wrapped core JSON line stream for callers that need it.
    pub fn into_inner(self) -> JsonLineStream<T> {
        self.inner
    }
}

impl<T> FuturesStream for JSONLDecoder<T>
where
    T: serde::de::DeserializeOwned + Unpin + Send,
{
    type Item = Result<T, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

/// Free-function equivalent of `JSONLDecoder::from_response()`.
pub fn from_response<T>(response: reqwest::Response) -> JSONLDecoder<T>
where
    T: serde::de::DeserializeOwned,
{
    JSONLDecoder::from_response(response)
}

/// TS-style camelCase alias for [`from_response`].
#[allow(non_snake_case)]
pub fn fromResponse<T>(response: reqwest::Response) -> JSONLDecoder<T>
where
    T: serde::de::DeserializeOwned,
{
    from_response(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Item {
        id: String,
    }

    #[tokio::test]
    async fn jsonl_decoder_decodes_lines_from_response_like_ts_from_response() {
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .header("content-type", "application/x-ndjson")
            .body("{\"id\":\"one\"}\n{\"id\":\"two\"}\n")
            .unwrap()
            .into();

        let mut decoder = JSONLDecoder::<Item>::fromResponse(response);
        assert_eq!(
            decoder.next().await.unwrap().unwrap(),
            Item { id: "one".into() }
        );
        assert_eq!(
            decoder.next().await.unwrap().unwrap(),
            Item { id: "two".into() }
        );
        assert!(decoder.next().await.is_none());
    }

    #[tokio::test]
    async fn jsonl_decoder_handles_partial_final_line_and_free_function_alias() {
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .body("{\"id\":\"final\"}")
            .unwrap()
            .into();

        let mut decoder = fromResponse::<Item>(response);
        assert_eq!(
            decoder.next().await.unwrap().unwrap(),
            Item { id: "final".into() }
        );
        assert!(decoder.next().await.is_none());
    }

    #[tokio::test]
    async fn jsonl_decoder_surfaces_json_parse_errors() {
        let response: reqwest::Response = http::Response::builder()
            .status(200)
            .body("not-json\n")
            .unwrap()
            .into();

        let mut decoder = JSONLDecoder::<Item>::from_response(response);
        let err = decoder.next().await.unwrap().unwrap_err();
        assert!(err
            .to_string()
            .contains("Failed to parse readable stream line as JSON"));
    }
}

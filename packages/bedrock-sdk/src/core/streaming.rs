// Maps to: TS packages/bedrock-sdk/src/core/streaming.ts
//
//! Amazon Bedrock InvokeModelWithResponseStream decoding.
//!
//! Bedrock streaming responses are AWS EventStream frames, not SSE frames.  The
//! TypeScript provider installs a Bedrock-specific `Stream` class that decodes
//! Smithy EventStream messages, extracts `chunk.bytes`, UTF-8 decodes those
//! bytes, and then JSON-deserializes each payload into the same items returned
//! by the core Anthropic streaming API.  This module provides the Rust
//! equivalent for the provider wrappers.

pub use anthropic_sdk::core::streaming::{
    _iterSSEMessages, _iter_sse_messages, fromReadableStream, fromSSEResponse,
    from_readable_stream, from_sse_response, iter_sse_messages, toReadableStream,
    to_readable_stream, JsonLineStream, RawSseStream, ServerSentEvent, SseStream,
};

use std::collections::{HashMap, VecDeque};
use std::pin::Pin;
use std::task::{Context, Poll};

use anthropic_sdk::core::error::ApiError;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use futures::{Stream as FuturesStream, StreamExt};
use pin_project_lite::pin_project;
use serde::de::DeserializeOwned;

pin_project! {
    /// Typed stream decoder for Bedrock `InvokeModelWithResponseStream` bodies.
    ///
    /// Maps to TS `packages/bedrock-sdk/src/core/streaming.ts` `Stream`.
    pub struct BedrockEventStream<T> {
        #[pin]
        body_stream: futures::stream::BoxStream<'static, reqwest::Result<Bytes>>,
        pending_bytes: Vec<u8>,
        pending_items: VecDeque<Result<T, ApiError>>,
        done: bool,
        _marker: std::marker::PhantomData<T>,
    }
}

impl<T> BedrockEventStream<T>
where
    T: DeserializeOwned,
{
    /// Create a Bedrock EventStream decoder from a `reqwest::Response`.
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            body_stream: response.bytes_stream().boxed(),
            pending_bytes: Vec::new(),
            pending_items: VecDeque::new(),
            done: false,
            _marker: std::marker::PhantomData,
        }
    }

    /// Bedrock override for TS `Stream.fromSSEResponse()`.
    ///
    /// Bedrock's HTTP response is Smithy EventStream rather than SSE; the name
    /// is kept for provider API parity with the TS class override.
    pub fn from_sse_response(response: reqwest::Response) -> Self {
        Self::new(response)
    }

    /// TS-style camelCase alias for [`BedrockEventStream::from_sse_response`].
    #[allow(non_snake_case)]
    pub fn fromSSEResponse(response: reqwest::Response) -> Self {
        Self::new(response)
    }

    fn drain_pending_bytes(
        pending_bytes: &mut Vec<u8>,
        pending_items: &mut VecDeque<Result<T, ApiError>>,
    ) -> Result<(), ApiError> {
        loop {
            if pending_bytes.len() < EVENT_STREAM_PRELUDE_LEN {
                return Ok(());
            }

            let total_len = u32::from_be_bytes([
                pending_bytes[0],
                pending_bytes[1],
                pending_bytes[2],
                pending_bytes[3],
            ]) as usize;

            if total_len < EVENT_STREAM_MIN_MESSAGE_LEN {
                return Err(ApiError::Sdk(format!(
                    "Malformed Bedrock event stream frame: total length {total_len} is smaller than minimum {EVENT_STREAM_MIN_MESSAGE_LEN}"
                )));
            }

            if pending_bytes.len() < total_len {
                return Ok(());
            }

            let message: Vec<u8> = pending_bytes.drain(..total_len).collect();
            if let Some(item) = decode_event_stream_message::<T>(&message)? {
                pending_items.push_back(item);
            }
        }
    }
}

/// TS provider export-name alias: `packages/bedrock-sdk/src/core/streaming.ts`
/// exports a Bedrock-specific `Stream` class that overrides
/// `fromSSEResponse()` to decode AWS EventStream frames.
pub type Stream<T> = BedrockEventStream<T>;

impl<T> FuturesStream for BedrockEventStream<T>
where
    T: DeserializeOwned + Unpin + Send,
{
    type Item = Result<T, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(item) = this.pending_items.pop_front() {
            return Poll::Ready(Some(item));
        }

        if *this.done {
            return Poll::Ready(None);
        }

        loop {
            match this.body_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending_bytes.extend_from_slice(&chunk);
                    if let Err(err) =
                        Self::drain_pending_bytes(this.pending_bytes, this.pending_items)
                    {
                        *this.done = true;
                        return Poll::Ready(Some(Err(err)));
                    }
                    if let Some(item) = this.pending_items.pop_front() {
                        return Poll::Ready(Some(item));
                    }
                }
                Poll::Ready(Some(Err(err))) => {
                    *this.done = true;
                    return Poll::Ready(Some(Err(ApiError::Connection {
                        message: format!("Bedrock event stream read error: {err}"),
                        cause: Some(Box::new(err)),
                    })));
                }
                Poll::Ready(None) => {
                    *this.done = true;
                    if !this.pending_bytes.is_empty() {
                        return Poll::Ready(Some(Err(ApiError::Sdk(format!(
                            "Malformed Bedrock event stream: {} trailing bytes after body ended",
                            this.pending_bytes.len()
                        )))));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Alias matching the TypeScript provider class name.
pub type StreamResponse<T> = BedrockEventStream<T>;

const EVENT_STREAM_PRELUDE_LEN: usize = 12;
const EVENT_STREAM_TRAILING_CRC_LEN: usize = 4;
const EVENT_STREAM_MIN_MESSAGE_LEN: usize =
    EVENT_STREAM_PRELUDE_LEN + EVENT_STREAM_TRAILING_CRC_LEN;

fn decode_event_stream_message<T>(message: &[u8]) -> Result<Option<Result<T, ApiError>>, ApiError>
where
    T: DeserializeOwned,
{
    if message.len() < EVENT_STREAM_MIN_MESSAGE_LEN {
        return Err(ApiError::Sdk(
            "Malformed Bedrock event stream frame: message is too short".into(),
        ));
    }

    let total_len = u32::from_be_bytes([message[0], message[1], message[2], message[3]]) as usize;
    let headers_len = u32::from_be_bytes([message[4], message[5], message[6], message[7]]) as usize;

    if total_len != message.len() {
        return Err(ApiError::Sdk(format!(
            "Malformed Bedrock event stream frame: declared length {total_len} does not match actual length {}",
            message.len()
        )));
    }
    validate_event_stream_crc(message)?;

    let payload_start = EVENT_STREAM_PRELUDE_LEN
        .checked_add(headers_len)
        .ok_or_else(|| {
            ApiError::Sdk("Malformed Bedrock event stream frame: headers length overflow".into())
        })?;
    let payload_end = total_len
        .checked_sub(EVENT_STREAM_TRAILING_CRC_LEN)
        .ok_or_else(|| {
            ApiError::Sdk("Malformed Bedrock event stream frame: missing trailing CRC".into())
        })?;

    if payload_start > payload_end {
        return Err(ApiError::Sdk(format!(
            "Malformed Bedrock event stream frame: headers length {headers_len} exceeds payload boundary"
        )));
    }

    let headers = parse_event_stream_headers(&message[EVENT_STREAM_PRELUDE_LEN..payload_start])?;
    let payload = &message[payload_start..payload_end];
    let event_type = headers
        .get(":event-type")
        .or_else(|| headers.get(":exception-type"))
        .map(String::as_str);

    match event_type {
        Some("chunk") => Ok(Some(decode_chunk_payload::<T>(payload))),
        Some("internalServerException") | Some("InternalServerException") => Ok(Some(Err(
            ApiError::generate(None, None, Some("InternalServerException".to_owned()), None),
        ))),
        Some("modelStreamErrorException") | Some("ModelStreamErrorException") => {
            Ok(Some(Err(ApiError::generate(
                None,
                None,
                Some("ModelStreamErrorException".to_owned()),
                None,
            ))))
        }
        Some("validationException") | Some("ValidationException") => Ok(Some(Err(
            ApiError::generate(None, None, Some("ValidationException".to_owned()), None),
        ))),
        Some("throttlingException") | Some("ThrottlingException") => Ok(Some(Err(
            ApiError::generate(None, None, Some("ThrottlingException".to_owned()), None),
        ))),
        _ => Ok(None),
    }
}

fn decode_chunk_payload<T>(payload: &[u8]) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    let json: serde_json::Value = serde_json::from_slice(payload).map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to parse Bedrock chunk wrapper as JSON: {err}"
        ))
    })?;
    let bytes = json
        .get("bytes")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::Sdk("Bedrock chunk payload is missing string field `bytes`".into())
        })?;
    let decoded = BASE64_STANDARD.decode(bytes).map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to base64-decode Bedrock chunk bytes: {err}"
        ))
    })?;
    // TextDecoder in the TS provider is non-fatal and replaces malformed
    // sequences, so use Rust's lossy UTF-8 conversion rather than rejecting
    // before JSON parsing.
    let decoded_json = String::from_utf8_lossy(&decoded);
    serde_json::from_str::<T>(&decoded_json).map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to parse Bedrock chunk bytes as JSON: {err}; raw: {decoded_json}"
        ))
    })
}

fn validate_event_stream_crc(message: &[u8]) -> Result<(), ApiError> {
    let expected_prelude_crc =
        u32::from_be_bytes([message[8], message[9], message[10], message[11]]);
    let actual_prelude_crc = crc32fast::hash(&message[..8]);
    if expected_prelude_crc != actual_prelude_crc {
        return Err(ApiError::Sdk(format!(
            "Malformed Bedrock event stream frame: prelude CRC mismatch (expected {expected_prelude_crc:#010x}, computed {actual_prelude_crc:#010x})"
        )));
    }

    let crc_offset = message.len() - EVENT_STREAM_TRAILING_CRC_LEN;
    let expected_message_crc = u32::from_be_bytes([
        message[crc_offset],
        message[crc_offset + 1],
        message[crc_offset + 2],
        message[crc_offset + 3],
    ]);
    let actual_message_crc = crc32fast::hash(&message[..crc_offset]);
    if expected_message_crc != actual_message_crc {
        return Err(ApiError::Sdk(format!(
            "Malformed Bedrock event stream frame: message CRC mismatch (expected {expected_message_crc:#010x}, computed {actual_message_crc:#010x})"
        )));
    }
    Ok(())
}

fn parse_event_stream_headers(bytes: &[u8]) -> Result<HashMap<String, String>, ApiError> {
    let mut headers = HashMap::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let name_len = read_u8(bytes, &mut index)? as usize;
        let name_bytes = read_exact(bytes, &mut index, name_len)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|err| {
                ApiError::Sdk(format!("Invalid Bedrock event stream header name: {err}"))
            })?
            .to_owned();
        let value_type = read_u8(bytes, &mut index)?;
        if let Some(value) = parse_header_value(bytes, &mut index, value_type)? {
            headers.insert(name, value);
        }
    }

    Ok(headers)
}

fn parse_header_value(
    bytes: &[u8],
    index: &mut usize,
    value_type: u8,
) -> Result<Option<String>, ApiError> {
    match value_type {
        // bool true / false have no payload.
        0 => Ok(Some("true".to_owned())),
        1 => Ok(Some("false".to_owned())),
        // byte
        2 => {
            let raw = read_u8(bytes, index)?;
            Ok(Some(raw.to_string()))
        }
        // int16
        3 => {
            let raw = read_exact(bytes, index, 2)?;
            Ok(Some(i16::from_be_bytes([raw[0], raw[1]]).to_string()))
        }
        // int32
        4 => {
            let raw = read_exact(bytes, index, 4)?;
            Ok(Some(
                i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]).to_string(),
            ))
        }
        // int64
        5 => {
            let raw = read_exact(bytes, index, 8)?;
            Ok(Some(
                i64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ])
                .to_string(),
            ))
        }
        // byte array
        6 => {
            let len = read_u16(bytes, index)? as usize;
            let _ = read_exact(bytes, index, len)?;
            Ok(None)
        }
        // string
        7 => {
            let len = read_u16(bytes, index)? as usize;
            let raw = read_exact(bytes, index, len)?;
            let value = std::str::from_utf8(raw)
                .map_err(|err| {
                    ApiError::Sdk(format!("Invalid Bedrock event stream header string: {err}"))
                })?
                .to_owned();
            Ok(Some(value))
        }
        // timestamp
        8 => {
            let raw = read_exact(bytes, index, 8)?;
            Ok(Some(
                i64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ])
                .to_string(),
            ))
        }
        // UUID
        9 => {
            let _ = read_exact(bytes, index, 16)?;
            Ok(None)
        }
        other => Err(ApiError::Sdk(format!(
            "Unsupported Bedrock event stream header value type: {other}"
        ))),
    }
}

fn read_u8(bytes: &[u8], index: &mut usize) -> Result<u8, ApiError> {
    Ok(*read_exact(bytes, index, 1)?
        .first()
        .ok_or_else(|| ApiError::Sdk("Unexpected empty byte read".into()))?)
}

fn read_u16(bytes: &[u8], index: &mut usize) -> Result<u16, ApiError> {
    let raw = read_exact(bytes, index, 2)?;
    Ok(u16::from_be_bytes([raw[0], raw[1]]))
}

fn read_exact<'a>(bytes: &'a [u8], index: &mut usize, len: usize) -> Result<&'a [u8], ApiError> {
    let end = index
        .checked_add(len)
        .ok_or_else(|| ApiError::Sdk("Bedrock event stream header length overflow".into()))?;
    if end > bytes.len() {
        return Err(ApiError::Sdk(format!(
            "Malformed Bedrock event stream header: need {len} bytes at offset {}, only {} bytes remain",
            *index,
            bytes.len().saturating_sub(*index)
        )));
    }
    let out = &bytes[*index..end];
    *index = end;
    Ok(out)
}

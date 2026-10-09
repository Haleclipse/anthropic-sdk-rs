// Maps to: TS packages/bedrock-sdk/src/AWS_restJson1.ts
//
//! Minimal AWS restJson1 EventStream deserializer used by the Bedrock provider.
//!
//! The TypeScript Bedrock package carries a trimmed copy of Smithy's generated
//! `Aws_restJson1.ts` so it can call `de_ResponseStream()` for
//! `InvokeModelWithResponseStream` responses. Rust's high-level provider stream
//! decoder lives in [`crate::core::streaming`]; this module exposes the lower
//! level raw Bedrock `ResponseStream` event surface for source-layout and helper
//! parity.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use anthropic_sdk::core::error::ApiError;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use pin_project_lite::pin_project;

/// Maps to AWS SDK `PayloadPart`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadPart {
    pub bytes: Vec<u8>,
}

/// Metadata fields Smithy attaches to service exceptions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponseMetadata {
    pub http_status_code: Option<u16>,
    pub request_id: Option<String>,
    pub extended_request_id: Option<String>,
    pub cf_id: Option<String>,
}

/// Common Bedrock EventStream exception payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BedrockEventStreamException {
    pub message: Option<String>,
    pub original_message: Option<String>,
    pub original_status_code: Option<i32>,
    pub metadata: ResponseMetadata,
}

/// Maps to AWS SDK `ResponseStream` union.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseStream {
    Chunk {
        chunk: PayloadPart,
    },
    InternalServerException {
        internal_server_exception: BedrockEventStreamException,
    },
    ModelStreamErrorException {
        model_stream_error_exception: BedrockEventStreamException,
    },
    ValidationException {
        validation_exception: BedrockEventStreamException,
    },
    ThrottlingException {
        throttling_exception: BedrockEventStreamException,
    },
    Unknown {
        event_type: Option<String>,
        payload: Vec<u8>,
    },
}

pin_project! {
    /// Stream returned by [`de_response_stream`].
    pub struct AwsRestJson1ResponseStream {
        #[pin]
        body_stream: futures::stream::BoxStream<'static, Result<Bytes, ApiError>>,
        pending_bytes: Vec<u8>,
        pending_events: VecDeque<Result<ResponseStream, ApiError>>,
        done: bool,
    }
}

impl Stream for AwsRestJson1ResponseStream {
    type Item = Result<ResponseStream, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(event) = this.pending_events.pop_front() {
            return Poll::Ready(Some(event));
        }

        if *this.done {
            return Poll::Ready(None);
        }

        loop {
            match this.body_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending_bytes.extend_from_slice(&chunk);
                    if let Err(err) = drain_pending_bytes(this.pending_bytes, this.pending_events) {
                        *this.done = true;
                        return Poll::Ready(Some(Err(err)));
                    }
                    if let Some(event) = this.pending_events.pop_front() {
                        return Poll::Ready(Some(event));
                    }
                }
                Poll::Ready(Some(Err(err))) => {
                    *this.done = true;
                    return Poll::Ready(Some(Err(err)));
                }
                Poll::Ready(None) => {
                    *this.done = true;
                    if !this.pending_bytes.is_empty() {
                        return Poll::Ready(Some(Err(ApiError::Sdk(format!(
                            "Malformed AWS restJson1 event stream: {} trailing bytes after body ended",
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

/// Deserialize an AWS restJson1 Bedrock `ResponseStream` EventStream.
///
/// Maps to TS `de_ResponseStream(output, context)`. Rust callers pass the raw
/// byte stream directly; base64 and UTF-8 helpers are built into this adapter.
pub fn de_response_stream<S, E>(stream: S) -> AwsRestJson1ResponseStream
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: fmt::Display + Send + Sync + 'static,
{
    AwsRestJson1ResponseStream {
        body_stream: stream
            .map(|chunk| {
                chunk.map_err(|err| ApiError::Connection {
                    message: format!("AWS restJson1 event stream read error: {err}"),
                    cause: None,
                })
            })
            .boxed(),
        pending_bytes: Vec::new(),
        pending_events: VecDeque::new(),
        done: false,
    }
}

/// TS-style alias for [`de_response_stream`].
#[allow(non_snake_case)]
pub fn de_ResponseStream<S, E>(stream: S) -> AwsRestJson1ResponseStream
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: fmt::Display + Send + Sync + 'static,
{
    de_response_stream(stream)
}

/// Convenience constructor from a `reqwest::Response` body.
pub fn from_response(response: reqwest::Response) -> AwsRestJson1ResponseStream {
    de_response_stream(response.bytes_stream())
}

/// TS-style camelCase alias for [`from_response`].
#[allow(non_snake_case)]
pub fn fromResponse(response: reqwest::Response) -> AwsRestJson1ResponseStream {
    from_response(response)
}

const EVENT_STREAM_PRELUDE_LEN: usize = 12;
const EVENT_STREAM_TRAILING_CRC_LEN: usize = 4;
const EVENT_STREAM_MIN_MESSAGE_LEN: usize =
    EVENT_STREAM_PRELUDE_LEN + EVENT_STREAM_TRAILING_CRC_LEN;

fn drain_pending_bytes(
    pending_bytes: &mut Vec<u8>,
    pending_events: &mut VecDeque<Result<ResponseStream, ApiError>>,
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
                "Malformed AWS restJson1 event stream frame: total length {total_len} is smaller than minimum {EVENT_STREAM_MIN_MESSAGE_LEN}"
            )));
        }

        if pending_bytes.len() < total_len {
            return Ok(());
        }

        let message: Vec<u8> = pending_bytes.drain(..total_len).collect();
        pending_events.push_back(decode_event_stream_message(&message));
    }
}

fn decode_event_stream_message(message: &[u8]) -> Result<ResponseStream, ApiError> {
    if message.len() < EVENT_STREAM_MIN_MESSAGE_LEN {
        return Err(ApiError::Sdk(
            "Malformed AWS restJson1 event stream frame: message is too short".into(),
        ));
    }

    let total_len = u32::from_be_bytes([message[0], message[1], message[2], message[3]]) as usize;
    let headers_len = u32::from_be_bytes([message[4], message[5], message[6], message[7]]) as usize;

    if total_len != message.len() {
        return Err(ApiError::Sdk(format!(
            "Malformed AWS restJson1 event stream frame: declared length {total_len} does not match actual length {}",
            message.len()
        )));
    }
    validate_event_stream_crc(message)?;

    let payload_start = EVENT_STREAM_PRELUDE_LEN
        .checked_add(headers_len)
        .ok_or_else(|| {
            ApiError::Sdk(
                "Malformed AWS restJson1 event stream frame: headers length overflow".into(),
            )
        })?;
    let payload_end = total_len
        .checked_sub(EVENT_STREAM_TRAILING_CRC_LEN)
        .ok_or_else(|| {
            ApiError::Sdk("Malformed AWS restJson1 event stream frame: missing trailing CRC".into())
        })?;

    if payload_start > payload_end {
        return Err(ApiError::Sdk(format!(
            "Malformed AWS restJson1 event stream frame: headers length {headers_len} exceeds payload boundary"
        )));
    }

    let headers = parse_event_stream_headers(&message[EVENT_STREAM_PRELUDE_LEN..payload_start])?;
    let payload = &message[payload_start..payload_end];
    let event_type = headers
        .get(":event-type")
        .or_else(|| headers.get(":exception-type"))
        .cloned();

    match event_type.as_deref() {
        Some("chunk") => Ok(ResponseStream::Chunk {
            chunk: decode_payload_part(payload)?,
        }),
        Some("internalServerException") | Some("InternalServerException") => {
            Ok(ResponseStream::InternalServerException {
                internal_server_exception: decode_exception(payload)?,
            })
        }
        Some("modelStreamErrorException") | Some("ModelStreamErrorException") => {
            Ok(ResponseStream::ModelStreamErrorException {
                model_stream_error_exception: decode_exception(payload)?,
            })
        }
        Some("validationException") | Some("ValidationException") => {
            Ok(ResponseStream::ValidationException {
                validation_exception: decode_exception(payload)?,
            })
        }
        Some("throttlingException") | Some("ThrottlingException") => {
            Ok(ResponseStream::ThrottlingException {
                throttling_exception: decode_exception(payload)?,
            })
        }
        _ => Ok(ResponseStream::Unknown {
            event_type,
            payload: payload.to_vec(),
        }),
    }
}

fn decode_payload_part(payload: &[u8]) -> Result<PayloadPart, ApiError> {
    let payload = String::from_utf8_lossy(payload);
    let json: serde_json::Value = serde_json::from_str(&payload).map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to parse AWS restJson1 PayloadPart wrapper as JSON: {err}"
        ))
    })?;
    let bytes = json
        .get("bytes")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::Sdk("AWS restJson1 PayloadPart is missing string field `bytes`".into())
        })?;
    let bytes = BASE64_STANDARD.decode(bytes).map_err(|err| {
        ApiError::Sdk(format!(
            "Failed to base64-decode AWS restJson1 PayloadPart bytes: {err}"
        ))
    })?;
    Ok(PayloadPart { bytes })
}

fn decode_exception(payload: &[u8]) -> Result<BedrockEventStreamException, ApiError> {
    let json: serde_json::Value = if payload.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        let payload = String::from_utf8_lossy(payload);
        serde_json::from_str(&payload).map_err(|err| {
            ApiError::Sdk(format!(
                "Failed to parse AWS restJson1 exception payload as JSON: {err}"
            ))
        })?
    };

    Ok(BedrockEventStreamException {
        message: json
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        original_message: json
            .get("originalMessage")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        original_status_code: json
            .get("originalStatusCode")
            .and_then(serde_json::Value::as_i64)
            .map(|value| value as i32),
        metadata: ResponseMetadata::default(),
    })
}

fn validate_event_stream_crc(message: &[u8]) -> Result<(), ApiError> {
    let expected_prelude_crc =
        u32::from_be_bytes([message[8], message[9], message[10], message[11]]);
    let actual_prelude_crc = crc32fast::hash(&message[..8]);
    if expected_prelude_crc != actual_prelude_crc {
        return Err(ApiError::Sdk(format!(
            "Malformed AWS restJson1 event stream frame: prelude CRC mismatch (expected {expected_prelude_crc:#010x}, computed {actual_prelude_crc:#010x})"
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
            "Malformed AWS restJson1 event stream frame: message CRC mismatch (expected {expected_message_crc:#010x}, computed {actual_message_crc:#010x})"
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
            .map_err(|err| ApiError::Sdk(format!("Invalid AWS restJson1 header name: {err}")))?
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
        0 => Ok(Some("true".to_owned())),
        1 => Ok(Some("false".to_owned())),
        2 => Ok(Some(read_u8(bytes, index)?.to_string())),
        3 => {
            let raw = read_exact(bytes, index, 2)?;
            Ok(Some(i16::from_be_bytes([raw[0], raw[1]]).to_string()))
        }
        4 => {
            let raw = read_exact(bytes, index, 4)?;
            Ok(Some(
                i32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]).to_string(),
            ))
        }
        5 => {
            let raw = read_exact(bytes, index, 8)?;
            Ok(Some(
                i64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ])
                .to_string(),
            ))
        }
        6 => {
            let len_bytes = read_exact(bytes, index, 2)?;
            let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;
            let raw = read_exact(bytes, index, len)?;
            Ok(Some(bytes_to_hex(raw)))
        }
        7 => {
            let len_bytes = read_exact(bytes, index, 2)?;
            let len = u16::from_be_bytes([len_bytes[0], len_bytes[1]]) as usize;
            let raw = read_exact(bytes, index, len)?;
            let value = std::str::from_utf8(raw).map_err(|err| {
                ApiError::Sdk(format!("Invalid AWS restJson1 header string: {err}"))
            })?;
            Ok(Some(value.to_owned()))
        }
        8 => {
            let raw = read_exact(bytes, index, 8)?;
            Ok(Some(
                i64::from_be_bytes([
                    raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                ])
                .to_string(),
            ))
        }
        9 => {
            let raw = read_exact(bytes, index, 16)?;
            Ok(Some(bytes_to_hex(raw)))
        }
        other => Err(ApiError::Sdk(format!(
            "Unsupported AWS restJson1 event stream header value type: {other}"
        ))),
    }
}

fn read_u8(bytes: &[u8], index: &mut usize) -> Result<u8, ApiError> {
    let byte = *bytes.get(*index).ok_or_else(|| {
        ApiError::Sdk(format!(
            "Malformed AWS restJson1 event stream header: missing byte at offset {}",
            *index
        ))
    })?;
    *index += 1;
    Ok(byte)
}

fn read_exact<'a>(bytes: &'a [u8], index: &mut usize, len: usize) -> Result<&'a [u8], ApiError> {
    let end = index
        .checked_add(len)
        .ok_or_else(|| ApiError::Sdk("AWS restJson1 event stream header length overflow".into()))?;
    let out = bytes.get(*index..end).ok_or_else(|| {
        ApiError::Sdk(format!(
            "Malformed AWS restJson1 event stream header: need {len} bytes at offset {}, only {} bytes remain",
            *index,
            bytes.len().saturating_sub(*index)
        ))
    })?;
    *index = end;
    Ok(out)
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

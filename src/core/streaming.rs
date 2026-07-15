// Maps to: TS core/streaming.ts
//
//! Server-Sent Events (SSE) stream parser for the Anthropic streaming API.
//!
//! Converts a raw `reqwest::Response` byte stream into a typed `futures::Stream`
//! of deserialized items, handling SSE framing, multi-byte UTF-8 across chunk
//! boundaries, and all Anthropic event types (message_start, content_block_delta,
//! ping, error, etc.).

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::{Stream as FuturesStream, StreamExt};
use pin_project_lite::pin_project;

use crate::core::error::ApiError;
use crate::internal::decoders::line::{find_double_newline_index, LineDecoder};

// ---------------------------------------------------------------------------
// ServerSentEvent
// ---------------------------------------------------------------------------

/// A single parsed Server-Sent Event.
///
/// Maps to: TS `ServerSentEvent` type in core/streaming.ts
#[derive(Debug, Clone)]
pub struct ServerSentEvent {
    /// The `event:` field, if present.
    pub event: Option<String>,
    /// The `data:` field(s), joined with `\n` when multiple `data:` lines exist.
    pub data: String,
    /// The raw lines that formed this event (before parsing).
    pub raw: Vec<String>,
}

// ---------------------------------------------------------------------------
// Known event names that carry a JSON payload
// ---------------------------------------------------------------------------

/// Returns `true` for event names whose `data:` field should be deserialized
/// as JSON and yielded to the caller.
fn is_message_event(event: &str) -> bool {
    matches!(
        event,
        "message_start"
            | "message_delta"
            | "message_stop"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "completion"
    )
}

// ---------------------------------------------------------------------------
// SseStream
// ---------------------------------------------------------------------------

pin_project! {
    /// An async `Stream` that reads SSE frames from a `reqwest::Response` byte
    /// stream and yields deserialized `T` items for each message event.
    ///
    /// Created via [`from_sse_response`].
    ///
    /// Maps to: TS `Stream.fromSSEResponse()` in core/streaming.ts
    pub struct SseStream<T> {
        #[pin]
        body_stream: futures::stream::BoxStream<'static, reqwest::Result<Bytes>>,
        pending_bytes: Vec<u8>,
        pending_events: Vec<ServerSentEvent>,
        line_decoder: LineDecoder,
        sse_decoder: SSEDecoder,
        done: bool,
        _marker: std::marker::PhantomData<T>,
    }
}

impl<T> SseStream<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Creates a new `SseStream` wrapping the given HTTP response.
    ///
    /// Maps to: TS `Stream.fromSSEResponse()` constructor path
    pub fn new(response: reqwest::Response) -> Self {
        let body_stream = response.bytes_stream().boxed();

        Self {
            body_stream,
            pending_bytes: Vec::new(),
            pending_events: Vec::new(),
            line_decoder: LineDecoder::new(),
            sse_decoder: SSEDecoder::new(),
            done: false,
            _marker: std::marker::PhantomData,
        }
    }

    /// Process accumulated bytes: split on double-newline boundaries, decode
    /// lines, decode SSE frames, and queue them in `pending_events`.
    fn drain_bytes(
        pending_bytes: &mut Vec<u8>,
        line_decoder: &mut LineDecoder,
        sse_decoder: &mut SSEDecoder,
        pending_events: &mut Vec<ServerSentEvent>,
    ) {
        // Yield complete SSE chunks (delimited by double newlines).
        while let Some(boundary) = find_double_newline_index(pending_bytes) {
            let chunk: Vec<u8> = pending_bytes.drain(..boundary).collect();
            let lines = line_decoder.decode(&chunk);
            for line in lines {
                if let Some(evt) = sse_decoder.decode(&line) {
                    pending_events.push(evt);
                }
            }
        }
    }

    /// Flush any remaining buffered data (called once the body stream ends).
    fn flush_remaining(
        pending_bytes: &mut Vec<u8>,
        line_decoder: &mut LineDecoder,
        sse_decoder: &mut SSEDecoder,
        pending_events: &mut Vec<ServerSentEvent>,
    ) {
        // Flush leftover bytes that did not end with a double newline.
        if !pending_bytes.is_empty() {
            let rest: Vec<u8> = std::mem::take(pending_bytes);
            let lines = line_decoder.decode(&rest);
            for line in lines {
                if let Some(evt) = sse_decoder.decode(&line) {
                    pending_events.push(evt);
                }
            }
        }

        // Flush the line decoder (may have a trailing partial line).
        let flushed = line_decoder.flush();
        for line in flushed {
            if let Some(evt) = sse_decoder.decode(&line) {
                pending_events.push(evt);
            }
        }

        // Flush the SSE decoder (may have accumulated data without a trailing
        // empty line).
        if let Some(evt) = sse_decoder.decode("") {
            pending_events.push(evt);
        }
    }
}

/// Try to consume one event from `pending_events` and convert it into a
/// stream item.  Returns `Some(Poll::Ready(...))` if an item was produced or
/// an error was encountered, or `None` if there are no actionable events.
fn sse_error_message(data: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(data) {
        Ok(value) => {
            if let Some(message) = value.get("message") {
                return match message {
                    serde_json::Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
            }

            // In the normal Anthropic SSE error shape there is no top-level
            // `message`, so TS falls back to JSON.stringify(error). Server SSE
            // JSON is already compact; preserving the raw string also preserves
            // field order for snapshot parity.
            data.to_owned()
        }
        Err(_) => serde_json::to_string(data).unwrap_or_else(|_| format!("\"{data}\"")),
    }
}

fn try_yield_event<T: serde::de::DeserializeOwned>(
    pending_events: &mut Vec<ServerSentEvent>,
) -> Option<Poll<Option<Result<T, ApiError>>>> {
    while !pending_events.is_empty() {
        let evt = pending_events.remove(0);

        // "event: error" -> surface as ApiError. TS constructs APIError
        // directly here instead of using APIError.generate(), so a JSON error
        // event's displayed message is the SSE body (or top-level `message`) even
        // without an HTTP status/headers pair.
        if evt.event.as_deref() == Some("error") {
            return Some(Poll::Ready(Some(Err(ApiError::Connection {
                message: sse_error_message(&evt.data),
                cause: None,
            }))));
        }

        // "event: ping" -> skip
        if evt.event.as_deref() == Some("ping") {
            continue;
        }

        // Message events -> deserialize JSON
        if let Some(ref event_name) = evt.event {
            if is_message_event(event_name) {
                return Some(match serde_json::from_str::<T>(&evt.data) {
                    Ok(item) => Poll::Ready(Some(Ok(item))),
                    Err(e) => Poll::Ready(Some(Err(ApiError::Sdk(format!(
                        "Failed to parse SSE data as JSON: {e}; raw: {:?}",
                        evt.raw
                    ))))),
                });
            }
        }

        // Unknown event type -- silently skip (matches TS behaviour where
        // unrecognised events fall through without yielding).
    }
    None
}

impl<T> FuturesStream for SseStream<T>
where
    T: serde::de::DeserializeOwned + Unpin + Send,
{
    type Item = Result<T, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // 1. Yield any buffered events first.
        if let Some(poll) = try_yield_event::<T>(this.pending_events) {
            return poll;
        }

        if *this.done {
            return Poll::Ready(None);
        }

        // 2. Poll the underlying byte stream for more data.
        loop {
            match this.body_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending_bytes.extend_from_slice(&chunk);
                    SseStream::<T>::drain_bytes(
                        this.pending_bytes,
                        this.line_decoder,
                        this.sse_decoder,
                        this.pending_events,
                    );
                    if let Some(poll) = try_yield_event::<T>(this.pending_events) {
                        return poll;
                    }
                    // No event ready yet -- loop to read more chunks.
                }
                Poll::Ready(Some(Err(e))) => {
                    *this.done = true;
                    return Poll::Ready(Some(Err(ApiError::Connection {
                        message: format!("Stream read error: {e}"),
                        cause: Some(Box::new(e)),
                    })));
                }
                Poll::Ready(None) => {
                    // Body exhausted -- flush remaining bytes.
                    *this.done = true;
                    SseStream::<T>::flush_remaining(
                        this.pending_bytes,
                        this.line_decoder,
                        this.sse_decoder,
                        this.pending_events,
                    );
                    if let Some(poll) = try_yield_event::<T>(this.pending_events) {
                        return poll;
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Raw SSE iterator (`_iterSSEMessages` parity)
// ---------------------------------------------------------------------------

pin_project! {
    /// Stream of raw server-sent events decoded from a response body.
    ///
    /// Maps to: TS `_iterSSEMessages(response, controller)` in
    /// `core/streaming.ts`. Rust does not need an explicit `AbortController`;
    /// dropping the stream drops the underlying response body.
    pub struct RawSseStream {
        #[pin]
        body_stream: futures::stream::BoxStream<'static, reqwest::Result<Bytes>>,
        pending_bytes: Vec<u8>,
        pending_events: Vec<ServerSentEvent>,
        line_decoder: LineDecoder,
        sse_decoder: SSEDecoder,
        done: bool,
    }
}

impl RawSseStream {
    /// Creates a raw SSE stream from an HTTP response.
    pub fn new(response: reqwest::Response) -> Self {
        Self {
            body_stream: response.bytes_stream().boxed(),
            pending_bytes: Vec::new(),
            pending_events: Vec::new(),
            line_decoder: LineDecoder::new(),
            sse_decoder: SSEDecoder::new(),
            done: false,
        }
    }

    fn drain_bytes(
        pending_bytes: &mut Vec<u8>,
        line_decoder: &mut LineDecoder,
        sse_decoder: &mut SSEDecoder,
        pending_events: &mut Vec<ServerSentEvent>,
    ) {
        while let Some(boundary) = find_double_newline_index(pending_bytes) {
            let chunk: Vec<u8> = pending_bytes.drain(..boundary).collect();
            let lines = line_decoder.decode(&chunk);
            for line in lines {
                if let Some(evt) = sse_decoder.decode(&line) {
                    pending_events.push(evt);
                }
            }
        }
    }

    fn flush_remaining(
        pending_bytes: &mut Vec<u8>,
        line_decoder: &mut LineDecoder,
        sse_decoder: &mut SSEDecoder,
        pending_events: &mut Vec<ServerSentEvent>,
    ) {
        if !pending_bytes.is_empty() {
            let rest: Vec<u8> = std::mem::take(pending_bytes);
            let lines = line_decoder.decode(&rest);
            for line in lines {
                if let Some(evt) = sse_decoder.decode(&line) {
                    pending_events.push(evt);
                }
            }
        }

        let flushed = line_decoder.flush();
        for line in flushed {
            if let Some(evt) = sse_decoder.decode(&line) {
                pending_events.push(evt);
            }
        }

        if let Some(evt) = sse_decoder.decode("") {
            pending_events.push(evt);
        }
    }
}

impl FuturesStream for RawSseStream {
    type Item = Result<ServerSentEvent, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if !this.pending_events.is_empty() {
            return Poll::Ready(Some(Ok(this.pending_events.remove(0))));
        }

        if *this.done {
            return Poll::Ready(None);
        }

        loop {
            match this.body_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending_bytes.extend_from_slice(&chunk);
                    RawSseStream::drain_bytes(
                        this.pending_bytes,
                        this.line_decoder,
                        this.sse_decoder,
                        this.pending_events,
                    );
                    if !this.pending_events.is_empty() {
                        return Poll::Ready(Some(Ok(this.pending_events.remove(0))));
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    *this.done = true;
                    return Poll::Ready(Some(Err(ApiError::Connection {
                        message: format!("Stream read error: {e}"),
                        cause: Some(Box::new(e)),
                    })));
                }
                Poll::Ready(None) => {
                    *this.done = true;
                    RawSseStream::flush_remaining(
                        this.pending_bytes,
                        this.line_decoder,
                        this.sse_decoder,
                        this.pending_events,
                    );
                    if !this.pending_events.is_empty() {
                        return Poll::Ready(Some(Ok(this.pending_events.remove(0))));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Creates a raw SSE event stream from a response body.
///
/// Maps to: TS `_iterSSEMessages(response, controller)`.
pub fn iter_sse_messages(response: reqwest::Response) -> RawSseStream {
    RawSseStream::new(response)
}

/// Rust snake_case alias that keeps TS's leading underscore naming intent.
pub fn _iter_sse_messages(response: reqwest::Response) -> RawSseStream {
    iter_sse_messages(response)
}

/// TS-style alias for [`iter_sse_messages`].
#[allow(non_snake_case)]
pub fn _iterSSEMessages(response: reqwest::Response) -> RawSseStream {
    iter_sse_messages(response)
}

// ---------------------------------------------------------------------------
// Public constructor (convenience alias)
// ---------------------------------------------------------------------------

/// Creates an [`SseStream`] from a `reqwest::Response` whose body is an SSE
/// byte stream.
///
/// This is a convenience wrapper around [`SseStream::new`].
///
/// Maps to: TS `Stream.fromSSEResponse()`
pub fn from_sse_response<T>(response: reqwest::Response) -> SseStream<T>
where
    T: serde::de::DeserializeOwned,
{
    SseStream::new(response)
}

/// TS-style camelCase alias for [`from_sse_response`].
#[allow(non_snake_case)]
pub fn fromSSEResponse<T>(response: reqwest::Response) -> SseStream<T>
where
    T: serde::de::DeserializeOwned,
{
    from_sse_response(response)
}

// ---------------------------------------------------------------------------
// JsonLineStream / from_readable_stream
// ---------------------------------------------------------------------------

pin_project! {
    /// A stream of newline-separated JSON values decoded from raw bytes.
    ///
    /// Maps to: TS `Stream.fromReadableStream()` in `core/streaming.ts`.
    /// Empty lines are skipped, each non-empty line is parsed as one JSON
    /// value, and UTF-8 / newline boundaries may span byte chunks.
    pub struct JsonLineStream<T> {
        #[pin]
        body_stream: futures::stream::BoxStream<'static, Result<Bytes, ApiError>>,
        pending_lines: Vec<String>,
        line_decoder: LineDecoder,
        done: bool,
        _marker: std::marker::PhantomData<T>,
    }
}

impl<T> JsonLineStream<T>
where
    T: serde::de::DeserializeOwned,
{
    /// Create a newline-JSON stream from an arbitrary byte stream.
    pub fn new<S, E>(stream: S) -> Self
    where
        S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let body_stream = stream
            .map(|chunk| {
                chunk.map_err(|err| ApiError::Connection {
                    message: format!("Stream read error: {err}"),
                    cause: Some(Box::new(err)),
                })
            })
            .boxed();

        Self {
            body_stream,
            pending_lines: Vec::new(),
            line_decoder: LineDecoder::new(),
            done: false,
            _marker: std::marker::PhantomData,
        }
    }
}

fn try_yield_json_line<T: serde::de::DeserializeOwned>(
    pending_lines: &mut Vec<String>,
) -> Option<Poll<Option<Result<T, ApiError>>>> {
    while !pending_lines.is_empty() {
        let line = pending_lines.remove(0);
        if line.is_empty() {
            continue;
        }

        return Some(Poll::Ready(Some(serde_json::from_str::<T>(&line).map_err(
            |err| {
                ApiError::Sdk(format!(
                    "Failed to parse readable stream line as JSON: {err}; raw: {line:?}"
                ))
            },
        ))));
    }
    None
}

impl<T> FuturesStream for JsonLineStream<T>
where
    T: serde::de::DeserializeOwned + Unpin + Send,
{
    type Item = Result<T, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if let Some(poll) = try_yield_json_line::<T>(this.pending_lines) {
            return poll;
        }

        if *this.done {
            return Poll::Ready(None);
        }

        loop {
            match this.body_stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.pending_lines.extend(this.line_decoder.decode(&chunk));
                    if let Some(poll) = try_yield_json_line::<T>(this.pending_lines) {
                        return poll;
                    }
                }
                Poll::Ready(Some(Err(err))) => {
                    *this.done = true;
                    return Poll::Ready(Some(Err(err)));
                }
                Poll::Ready(None) => {
                    *this.done = true;
                    this.pending_lines.extend(this.line_decoder.flush());
                    if let Some(poll) = try_yield_json_line::<T>(this.pending_lines) {
                        return poll;
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Creates a [`JsonLineStream`] from a newline-separated byte stream.
///
/// Maps to: TS `Stream.fromReadableStream()`.
pub fn from_readable_stream<T, S, E>(stream: S) -> JsonLineStream<T>
where
    T: serde::de::DeserializeOwned,
    S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    JsonLineStream::new(stream)
}

/// TS-style camelCase alias for [`from_readable_stream`].
#[allow(non_snake_case)]
pub fn fromReadableStream<T, S, E>(stream: S) -> JsonLineStream<T>
where
    T: serde::de::DeserializeOwned,
    S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    from_readable_stream(stream)
}

/// Converts a stream of JSON-serializable values into newline-delimited JSON
/// bytes that can be consumed again with [`from_readable_stream`].
///
/// This is the Rust equivalent of TS `Stream.toReadableStream()`: every item is
/// `JSON.stringify(value) + "\n"`, and upstream stream errors are propagated.
pub fn to_readable_stream<T, S>(stream: S) -> impl FuturesStream<Item = Result<Bytes, ApiError>>
where
    T: serde::Serialize,
    S: FuturesStream<Item = Result<T, ApiError>>,
{
    stream.map(|item| {
        item.and_then(|value| {
            serde_json::to_vec(&value)
                .map(|mut bytes| {
                    bytes.push(b'\n');
                    Bytes::from(bytes)
                })
                .map_err(|err| ApiError::Sdk(format!("failed to serialize stream item: {err}")))
        })
    })
}

/// TS-style camelCase alias for [`to_readable_stream`].
#[allow(non_snake_case)]
pub fn toReadableStream<T, S>(stream: S) -> impl FuturesStream<Item = Result<Bytes, ApiError>>
where
    T: serde::Serialize,
    S: FuturesStream<Item = Result<T, ApiError>>,
{
    to_readable_stream(stream)
}

/// Namespace-style utility matching TS `core/streaming.Stream` static helpers.
///
/// Rust's actual stream trait comes from `futures`; this zero-sized type exists
/// so callers can write `core::streaming::Stream::fromSSEResponse(response)` or
/// `Stream::fromReadableStream(bytes)` in addition to the free-function helpers.
pub struct Stream;

impl Stream {
    /// Associated-function equivalent of TS `Stream.fromSSEResponse()`.
    pub fn from_sse_response<T>(response: reqwest::Response) -> SseStream<T>
    where
        T: serde::de::DeserializeOwned,
    {
        from_sse_response(response)
    }

    /// TS-style camelCase associated alias for [`Stream::from_sse_response`].
    #[allow(non_snake_case)]
    pub fn fromSSEResponse<T>(response: reqwest::Response) -> SseStream<T>
    where
        T: serde::de::DeserializeOwned,
    {
        from_sse_response(response)
    }

    /// Associated-function equivalent of TS `Stream.fromReadableStream()`.
    pub fn from_readable_stream<T, S, E>(stream: S) -> JsonLineStream<T>
    where
        T: serde::de::DeserializeOwned,
        S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        from_readable_stream(stream)
    }

    /// TS-style camelCase associated alias for [`Stream::from_readable_stream`].
    #[allow(non_snake_case)]
    pub fn fromReadableStream<T, S, E>(stream: S) -> JsonLineStream<T>
    where
        T: serde::de::DeserializeOwned,
        S: FuturesStream<Item = Result<Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        from_readable_stream(stream)
    }

    /// Associated-function equivalent of TS `Stream.toReadableStream()`.
    pub fn to_readable_stream<T, S>(stream: S) -> impl FuturesStream<Item = Result<Bytes, ApiError>>
    where
        T: serde::Serialize,
        S: FuturesStream<Item = Result<T, ApiError>>,
    {
        to_readable_stream(stream)
    }

    /// TS-style camelCase associated alias for [`Stream::to_readable_stream`].
    #[allow(non_snake_case)]
    pub fn toReadableStream<T, S>(stream: S) -> impl FuturesStream<Item = Result<Bytes, ApiError>>
    where
        T: serde::Serialize,
        S: FuturesStream<Item = Result<T, ApiError>>,
    {
        to_readable_stream(stream)
    }
}

// ---------------------------------------------------------------------------
// SSEDecoder
// ---------------------------------------------------------------------------

/// Accumulates SSE field lines and emits a `ServerSentEvent` on each empty
/// line (the SSE frame boundary).
///
/// Maps to: TS `SSEDecoder` class in core/streaming.ts
struct SSEDecoder {
    data: Vec<String>,
    event: Option<String>,
    chunks: Vec<String>,
}

impl SSEDecoder {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            event: None,
            chunks: Vec::new(),
        }
    }

    /// Feed a single line to the decoder.  Returns `Some(ServerSentEvent)` when
    /// an empty line signals the end of a frame.
    ///
    /// Maps to: TS `SSEDecoder.decode()`
    fn decode(&mut self, line: &str) -> Option<ServerSentEvent> {
        // Strip a trailing \r (may happen depending on line-ending style).
        let line = line.strip_suffix('\r').unwrap_or(line);

        // Empty line -> emit accumulated event (if any).
        if line.is_empty() {
            if self.event.is_none() && self.data.is_empty() {
                return None;
            }

            let sse = ServerSentEvent {
                event: self.event.take(),
                data: self.data.join("\n"),
                raw: std::mem::take(&mut self.chunks),
            };

            self.data.clear();

            return Some(sse);
        }

        self.chunks.push(line.to_string());

        // Comment line -- ignore.
        if line.starts_with(':') {
            return None;
        }

        // Split on the first `:` into fieldname / value.
        let (fieldname, value) = match line.find(':') {
            Some(idx) => {
                let f = &line[..idx];
                let mut v = &line[idx + 1..];
                // Strip a single leading space from the value.
                if let Some(stripped) = v.strip_prefix(' ') {
                    v = stripped;
                }
                (f, v)
            }
            None => (line, ""),
        };

        match fieldname {
            "event" => {
                self.event = Some(value.to_string());
            }
            "data" => {
                self.data.push(value.to_string());
            }
            _ => {
                // Other fields (id, retry, etc.) are ignored per the SSE spec
                // for our use case.
            }
        }

        None
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internal::decoders::line::LineDecoder;

    // ---- SSEDecoder tests ----

    #[test]
    fn sse_decoder_simple_event() {
        let mut dec = SSEDecoder::new();
        assert!(dec.decode("event: message_start").is_none());
        assert!(dec.decode("data: {\"type\":\"message_start\"}").is_none());
        let evt = dec.decode("").expect("should emit on empty line");
        assert_eq!(evt.event.as_deref(), Some("message_start"));
        assert_eq!(evt.data, "{\"type\":\"message_start\"}");
        assert_eq!(evt.raw.len(), 2);
    }

    #[test]
    fn sse_decoder_multi_data_lines() {
        let mut dec = SSEDecoder::new();
        dec.decode("event: completion");
        dec.decode("data: line1");
        dec.decode("data: line2");
        let evt = dec.decode("").expect("should emit");
        assert_eq!(evt.data, "line1\nline2");
    }

    #[test]
    fn sse_decoder_comment_skipped() {
        let mut dec = SSEDecoder::new();
        assert!(dec.decode(": this is a comment").is_none());
        assert!(dec.decode("").is_none()); // no data accumulated
    }

    #[test]
    fn sse_decoder_empty_line_no_data() {
        let mut dec = SSEDecoder::new();
        assert!(dec.decode("").is_none());
    }

    #[test]
    fn sse_decoder_trailing_cr_stripped() {
        let mut dec = SSEDecoder::new();
        dec.decode("event: ping\r");
        let evt = dec.decode("").expect("should emit");
        assert_eq!(evt.event.as_deref(), Some("ping"));
    }

    #[test]
    fn sse_decoder_field_without_colon() {
        let mut dec = SSEDecoder::new();
        // A line with no colon at all -- fieldname is the whole line, value is "".
        dec.decode("event: completion");
        dec.decode("justfieldname");
        let evt = dec.decode("").expect("should emit");
        assert_eq!(evt.event.as_deref(), Some("completion"));
        assert!(evt.data.is_empty());
    }

    #[test]
    fn sse_decoder_data_no_space_after_colon() {
        let mut dec = SSEDecoder::new();
        dec.decode("event: message_start");
        dec.decode("data:nospace");
        let evt = dec.decode("").expect("should emit");
        assert_eq!(evt.data, "nospace");
    }

    // -- SSE end-to-end tests (streaming decoding) --

    /// Helper: push raw SSE bytes through LineDecoder -> SSEDecoder, returning all events.
    fn decode_sse_from_bytes(input: &[u8]) -> Vec<ServerSentEvent> {
        let mut line_decoder = LineDecoder::new();
        let mut sse_decoder = SSEDecoder::new();
        let mut events = Vec::new();

        let lines = line_decoder.decode(input);
        for line in &lines {
            if let Some(evt) = sse_decoder.decode(line) {
                events.push(evt);
            }
        }

        // Flush remaining
        let flushed = line_decoder.flush();
        for line in &flushed {
            if let Some(evt) = sse_decoder.decode(line) {
                events.push(evt);
            }
        }

        // Final empty-line flush for SSE decoder
        if let Some(evt) = sse_decoder.decode("") {
            events.push(evt);
        }

        events
    }

    #[test]
    fn sse_e2e_basic() {
        let input = b"event: completion\ndata: {\"foo\":true}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("completion"));
        let parsed: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(parsed, serde_json::json!({"foo": true}));
    }

    #[test]
    fn sse_e2e_data_without_event() {
        let input = b"data: {\"foo\":true}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert!(events[0].event.is_none());
        let parsed: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(parsed, serde_json::json!({"foo": true}));
    }

    #[test]
    fn sse_e2e_event_without_data() {
        let input = b"event: foo\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("foo"));
        assert_eq!(events[0].data, "");
    }

    #[test]
    fn sse_e2e_multiple_events() {
        let input = b"event: foo\n\nevent: ping\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("foo"));
        assert_eq!(events[0].data, "");
        assert_eq!(events[1].event.as_deref(), Some("ping"));
        assert_eq!(events[1].data, "");
    }

    #[test]
    fn sse_e2e_multiple_events_with_data() {
        let input = b"event: foo\ndata: {\"foo\":true}\n\nevent: ping\ndata: {\"bar\":false}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("foo"));
        let p0: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(p0, serde_json::json!({"foo": true}));
        assert_eq!(events[1].event.as_deref(), Some("ping"));
        let p1: serde_json::Value = serde_json::from_str(&events[1].data).unwrap();
        assert_eq!(p1, serde_json::json!({"bar": false}));
    }

    #[test]
    fn sse_e2e_multiple_data_lines_with_empty_line() {
        // Five data: lines, some with space, some without. Joined with \n.
        let input = b"event: completion\ndata: {\ndata: \"foo\":\ndata:\ndata:\ndata: true}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\n\"foo\":\n\n\ntrue}");
        let parsed: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(parsed, serde_json::json!({"foo": true}));
    }

    #[test]
    fn sse_e2e_json_escaped_double_newline() {
        // JSON-escaped \\n\\n inside data value should NOT split the SSE frame.
        let input = b"event: completion\ndata: {\"foo\": \"bar\\n\\nbaz\"}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        let parsed: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(parsed["foo"], "bar\n\nbaz");
    }

    #[test]
    fn sse_e2e_special_new_line_characters() {
        // U+2028 (LINE SEPARATOR, bytes 0xE2 0x80 0xA8) should NOT be treated as newline
        let line_sep = "\u{2028}";
        let input = format!(
            "event: completion\ndata: first\n\nevent: completion\ndata: {line_sep}\n\nevent: completion\ndata: third\n\n"
        );
        let events = decode_sse_from_bytes(input.as_bytes());
        assert_eq!(events.len(), 3);
        assert!(events[1].data.contains('\u{2028}'));
    }

    #[test]
    fn sse_e2e_multibyte_characters_across_chunks() {
        // Cyrillic string 'известни' split across chunks at multi-byte boundaries
        let full_data =
            "{\"content\":\"\u{0438}\u{0437}\u{0432}\u{0435}\u{0441}\u{0442}\u{043D}\u{0438}\"}";
        let input = format!("event: completion\ndata: {full_data}\n\n");
        let bytes = input.as_bytes();

        // Split at a multi-byte boundary (each Cyrillic char is 2 bytes in UTF-8)
        let split_point = bytes.len() / 2;
        let chunk1 = &bytes[..split_point];
        let chunk2 = &bytes[split_point..];

        let mut line_decoder = LineDecoder::new();
        let mut sse_decoder = SSEDecoder::new();
        let mut events = Vec::new();

        let lines1 = line_decoder.decode(chunk1);
        for line in &lines1 {
            if let Some(evt) = sse_decoder.decode(line) {
                events.push(evt);
            }
        }
        let lines2 = line_decoder.decode(chunk2);
        for line in &lines2 {
            if let Some(evt) = sse_decoder.decode(line) {
                events.push(evt);
            }
        }
        let flushed = line_decoder.flush();
        for line in &flushed {
            if let Some(evt) = sse_decoder.decode(line) {
                events.push(evt);
            }
        }
        if let Some(evt) = sse_decoder.decode("") {
            events.push(evt);
        }

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("completion"));
        let parsed: serde_json::Value = serde_json::from_str(&events[0].data).unwrap();
        assert_eq!(
            parsed["content"].as_str().unwrap(),
            "\u{0438}\u{0437}\u{0432}\u{0435}\u{0441}\u{0442}\u{043D}\u{0438}"
        );
    }

    // -- SSE error event test --

    #[test]
    fn sse_error_event_produces_api_error() {
        let input =
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("error"));

        // Verify that try_yield_event converts it to a connection-shaped SDK
        // error whose displayed message matches TS Stream.fromSSEResponse's
        // direct APIError construction from the SSE JSON body.
        let mut pending = events;
        let result = try_yield_event::<serde_json::Value>(&mut pending);
        match result {
            Some(std::task::Poll::Ready(Some(Err(e)))) => {
                assert!(matches!(e, ApiError::Connection { .. }));
                assert_eq!(
                    e.to_string(),
                    r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#
                );
            }
            other => panic!("expected ApiError::Connection, got {:?}", other),
        }
    }

    #[test]
    fn sse_error_event_fallback_to_raw_data() {
        // When the error data is not valid JSON, TS passes the raw string as
        // the APIError `error` value, which makes APIError.makeMessage use
        // JSON.stringify(string).
        let input = b"event: error\ndata: some raw error text\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);

        let mut pending = events;
        let result = try_yield_event::<serde_json::Value>(&mut pending);
        match result {
            Some(std::task::Poll::Ready(Some(Err(e)))) => {
                assert!(matches!(e, ApiError::Connection { .. }));
                assert_eq!(e.to_string(), r#""some raw error text""#);
            }
            other => panic!("expected ApiError::Connection, got {:?}", other),
        }
    }

    // -- ping event skipped test (TS: streaming.test.ts handles ping in Stream) --

    #[test]
    fn sse_ping_event_skipped_in_try_yield() {
        // A "ping" event should be silently skipped by try_yield_event,
        // matching the TS SDK behaviour where ping events are consumed
        // without being yielded to the caller.
        let input = b"event: ping\ndata: {}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("ping"));

        let mut pending = events;
        let result = try_yield_event::<serde_json::Value>(&mut pending);
        // Should return None (no actionable event), not an item.
        assert!(
            result.is_none(),
            "ping event should be skipped, got: {:?}",
            result
        );
        assert!(pending.is_empty(), "pending should be drained");
    }

    // -- unknown event skipped test --

    #[test]
    fn sse_unknown_event_skipped_in_try_yield() {
        // Events with unrecognised names (not message events, not error,
        // not ping) should be silently skipped, matching the TS SDK
        // behaviour where unrecognised events fall through.
        let input = b"event: some_unknown_event\ndata: {\"x\":1}\n\n";
        let events = decode_sse_from_bytes(input);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("some_unknown_event"));

        let mut pending = events;
        let result = try_yield_event::<serde_json::Value>(&mut pending);
        assert!(
            result.is_none(),
            "unknown event should be skipped, got: {:?}",
            result
        );
        assert!(pending.is_empty(), "pending should be drained");
    }
}

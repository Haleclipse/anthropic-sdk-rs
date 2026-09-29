// Maps to: TS lib/MessageStream.ts
//
// High-level wrapper around the SSE event stream that:
// 1. Accumulates raw streaming events into a running `Message` snapshot
// 2. Implements `futures::Stream<Item = Result<MessageStreamEvent, ApiError>>`
// 3. Provides convenience helpers (`final_message`, `final_text`, `abort`)

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::Stream;
use pin_project_lite::pin_project;

use crate::core::error::ApiError;
use crate::resources::messages::{
    ContentBlock, ContentBlockDelta, Message, MessageDeltaUsage, MessageStreamEvent,
};
use crate::sdk_lib::parser::{parse_message, ParsedMessage};
use crate::vendor::partial_json_parser::partial_parse;

// ---------------------------------------------------------------------------
// TracksToolInput
// ---------------------------------------------------------------------------

/// Content block variants whose `input` is incrementally reconstructed from
/// `input_json_delta` stream chunks.
///
/// Maps to TS `export type TracksToolInput = ToolUseBlock | ServerToolUseBlock`
/// in `lib/MessageStream.ts`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TracksToolInput {
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },

    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },
}

impl TracksToolInput {
    /// Borrow the tool input JSON, matching the common TS union property.
    pub fn input(&self) -> &serde_json::Value {
        match self {
            TracksToolInput::ToolUse { input, .. }
            | TracksToolInput::ServerToolUse { input, .. } => input,
        }
    }

    /// Borrow the tool-use id, matching the common TS union property.
    pub fn id(&self) -> &str {
        match self {
            TracksToolInput::ToolUse { id, .. } | TracksToolInput::ServerToolUse { id, .. } => id,
        }
    }

    /// Borrow the tool name, matching the common TS union property.
    pub fn name(&self) -> &str {
        match self {
            TracksToolInput::ToolUse { name, .. } | TracksToolInput::ServerToolUse { name, .. } => {
                name
            }
        }
    }
}

/// Rust equivalent of TS `tracksToolInput(content)` guard.
pub fn tracks_tool_input(content: &ContentBlock) -> Option<TracksToolInput> {
    match content {
        ContentBlock::ToolUse { id, input, name } => Some(TracksToolInput::ToolUse {
            id: id.clone(),
            input: input.clone(),
            name: name.clone(),
        }),
        ContentBlock::ServerToolUse { id, input, name } => Some(TracksToolInput::ServerToolUse {
            id: id.clone(),
            input: input.clone(),
            name: name.clone(),
        }),
        _ => None,
    }
}

impl<'a> TryFrom<&'a ContentBlock> for TracksToolInput {
    type Error = ApiError;

    fn try_from(value: &'a ContentBlock) -> Result<Self, Self::Error> {
        tracks_tool_input(value).ok_or_else(|| {
            ApiError::Sdk("content block does not contain stream-tracked tool input".to_owned())
        })
    }
}

// ---------------------------------------------------------------------------
// Stream event aliases
// ---------------------------------------------------------------------------

/// Rust stream-item counterpart to TS `MessageStreamEvents<ParsedT>`.
///
/// TypeScript exposes an event-listener map, while Rust exposes the same
/// lifecycle through `Stream<Item = MessageStreamEvent>`. This alias preserves
/// the exported TS name at the corresponding module path.
pub type MessageStreamEvents = MessageStreamEvent;

// ---------------------------------------------------------------------------
// SseStream type alias
// ---------------------------------------------------------------------------

/// A boxed, pinned, `Send`-able stream of SSE events.
///
/// Produced by the lower-level HTTP/SSE layer (`crate::core::streaming`) and
/// consumed by [`MessageStream`].
pub type SseStream =
    Pin<Box<dyn Stream<Item = Result<MessageStreamEvent, ApiError>> + Send + 'static>>;

// ---------------------------------------------------------------------------
// MessageStream
// ---------------------------------------------------------------------------

pin_project! {
    /// Wraps an SSE event stream and accumulates events into a `Message` snapshot.
    ///
    /// Implements `futures::Stream` so callers can iterate over individual
    /// [`MessageStreamEvent`] values while the struct silently keeps the running
    /// `Message` up-to-date.
    ///
    /// Maps to: TS `MessageStream`
    pub struct MessageStream {
        #[pin]
        inner: SseStream,
        current_message: Option<Message>,
        received_messages: Vec<Message>,
        aborted: bool,
        // Tracks the raw JSON string buffer per content-block index so that
        // `input_json_delta` chunks can be re-parsed via `partial_parse` on
        // every delta. Maps to the TS `JSON_BUF_PROPERTY` hidden property.
        json_bufs: HashMap<usize, String>,
    }
}

impl MessageStream {
    // -- Construction --------------------------------------------------------

    /// Create a new `MessageStream` from a lower-level SSE stream.
    ///
    /// Maps to: TS `new MessageStream()`
    pub fn new(stream: SseStream) -> Self {
        Self {
            inner: stream,
            current_message: None,
            received_messages: Vec::new(),
            aborted: false,
            json_bufs: HashMap::new(),
        }
    }

    /// Reconstruct a high-level message stream from newline-delimited JSON
    /// stream-event bytes.
    ///
    /// Maps to: TS `MessageStream.fromReadableStream()`.
    pub fn from_readable_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let events =
            crate::core::streaming::from_readable_stream::<MessageStreamEvent, _, _>(stream);
        Self::new(Box::pin(events))
    }

    /// TS-style camelCase alias for [`MessageStream::from_readable_stream`].
    #[allow(non_snake_case)]
    pub fn fromReadableStream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::from_readable_stream(stream)
    }

    // -- Accessors -----------------------------------------------------------

    /// Returns a reference to the current in-progress message snapshot, if any.
    ///
    /// Maps to: TS `get currentMessage()`
    pub fn current_message(&self) -> Option<&Message> {
        self.current_message.as_ref()
    }

    /// Returns a slice of all fully-received messages so far.
    ///
    /// Maps to: TS `receivedMessages`
    pub fn received_messages(&self) -> &[Message] {
        &self.received_messages
    }

    /// Returns `true` if [`abort`](Self::abort) has been called.
    ///
    /// Maps to: TS `get aborted()`
    pub fn aborted(&self) -> bool {
        self.aborted
    }

    // -- Abort ---------------------------------------------------------------

    /// Signal the stream to stop producing events.
    ///
    /// After calling this the `Stream` impl will immediately return
    /// `Poll::Ready(None)`.
    ///
    /// Maps to: TS `abort()`
    pub fn abort(&mut self) {
        self.aborted = true;
    }

    // -- Terminal helpers ----------------------------------------------------

    /// Consumes the stream, drives it to completion, and returns the last
    /// fully-received assistant `Message`.
    ///
    /// Returns `Err` if the stream ends without producing any message with
    /// `role=assistant`, or if the underlying stream yields an error.
    ///
    /// Maps to: TS `async done()`.
    pub async fn done(mut self) -> Result<(), ApiError> {
        use futures::StreamExt;
        while let Some(result) = self.next().await {
            // Propagate any stream error immediately.
            result?;
        }
        Ok(())
    }

    /// Consumes the stream, drives it to completion, and serializes each raw
    /// event as newline-delimited JSON bytes.
    ///
    /// Maps to: TS `MessageStream.toReadableStream()`.
    pub fn to_readable_stream(self) -> impl Stream<Item = Result<bytes::Bytes, ApiError>> {
        crate::core::streaming::to_readable_stream(self)
    }

    /// TS-style camelCase alias for [`MessageStream::to_readable_stream`].
    #[allow(non_snake_case)]
    pub fn toReadableStream(self) -> impl Stream<Item = Result<bytes::Bytes, ApiError>> {
        self.to_readable_stream()
    }

    /// Maps to: TS `async finalMessage()`
    pub async fn final_message(mut self) -> Result<Message, ApiError> {
        use futures::StreamExt;
        while let Some(result) = self.next().await {
            // Propagate any stream error immediately.
            result?;
        }
        self.received_messages.pop().ok_or_else(|| {
            ApiError::Sdk("stream ended without producing a Message with role=assistant".to_owned())
        })
    }

    /// TS-style camelCase alias for [`MessageStream::final_message`].
    #[allow(non_snake_case)]
    pub async fn finalMessage(self) -> Result<Message, ApiError> {
        self.final_message().await
    }

    /// Consumes the stream, drives it to completion, and returns the final
    /// message parsed as structured output.
    ///
    /// TypeScript's `MessageStream<ParsedT>.finalMessage()` returns a parsed
    /// message when the stream was created with an auto-parseable output
    /// format. Rust streams are not generic, so callers opt in explicitly with
    /// this helper and the target serde type `T`.
    pub async fn final_parsed_message<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<ParsedMessage<T>, ApiError> {
        let message = self.final_message().await?;
        parse_message(&message)
    }

    /// TS-style camelCase alias for [`MessageStream::final_parsed_message`].
    #[allow(non_snake_case)]
    pub async fn finalParsedMessage<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<ParsedMessage<T>, ApiError> {
        self.final_parsed_message().await
    }

    /// Consumes the stream, drives it to completion, and returns the
    /// concatenated text from all `text` content blocks of the final message.
    ///
    /// Multiple text blocks are joined with a single space (matching the TS
    /// SDK behaviour).
    ///
    /// Returns `Err` if the stream ends without producing a text block, or if
    /// the underlying stream yields an error.
    ///
    /// Maps to: TS `async finalText()`
    pub async fn final_text(self) -> Result<String, ApiError> {
        let message = self.final_message().await?;
        let texts: Vec<&str> = message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        if texts.is_empty() {
            return Err(ApiError::Sdk(
                "stream ended without producing a content block with type=text".to_owned(),
            ));
        }

        Ok(texts.join(" "))
    }

    /// TS-style camelCase alias for [`MessageStream::final_text`].
    #[allow(non_snake_case)]
    pub async fn finalText(self) -> Result<String, ApiError> {
        self.final_text().await
    }
}

// ---------------------------------------------------------------------------
// Stream impl
// ---------------------------------------------------------------------------

impl Stream for MessageStream {
    type Item = Result<MessageStreamEvent, ApiError>;

    /// Polls the underlying SSE stream for the next event.
    ///
    /// On each successful event the internal message snapshot is updated via
    /// the accumulation logic before the event is yielded to the caller.
    ///
    /// If the stream has been [`abort`](MessageStream::abort)ed, returns
    /// `Poll::Ready(None)` immediately.
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if *this.aborted {
            return Poll::Ready(None);
        }

        match this.inner.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(Some(Ok(event))) => {
                // Accumulate directly on the projected fields so we avoid
                // borrow conflicts with `&mut self`.
                match accumulate_into(
                    this.current_message,
                    this.received_messages,
                    this.json_bufs,
                    &event,
                ) {
                    Ok(()) => Poll::Ready(Some(Ok(event))),
                    Err(err) => Poll::Ready(Some(Err(err))),
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // SSE streams are unbounded; we cannot predict remaining events.
        (0, None)
    }
}

// ---------------------------------------------------------------------------
// Core accumulation logic (free functions)
// ---------------------------------------------------------------------------
//
// Factored out of `MessageStream` so that both `accumulate_event` (called via
// `&mut self` in `final_message`) and the `poll_next` path (which only has
// projected field borrows) can share the same logic without duplication.

/// Maps to: TS `#accumulateMessage()` -- operates on individual fields rather
/// than `&mut MessageStream` to avoid borrow conflicts in `poll_next`.
fn accumulate_into(
    current_message: &mut Option<Message>,
    received_messages: &mut Vec<Message>,
    json_bufs: &mut HashMap<usize, String>,
    event: &MessageStreamEvent,
) -> Result<(), ApiError> {
    match event {
        // -- message_start ---------------------------------------------------
        MessageStreamEvent::MessageStart { message } => {
            json_bufs.clear();
            *current_message = Some(message.clone());
        }

        // -- message_delta ---------------------------------------------------
        MessageStreamEvent::MessageDelta { delta, usage } => {
            if let Some(msg) = current_message {
                // Unconditionally assign to match TS SDK behaviour.
                msg.stop_reason = delta.stop_reason.clone();
                msg.stop_sequence = delta.stop_sequence.clone();
                apply_usage_delta(&mut msg.usage, usage);
            }
        }

        // -- content_block_start ---------------------------------------------
        MessageStreamEvent::ContentBlockStart {
            content_block,
            index: _,
        } => {
            if let Some(msg) = current_message {
                msg.content.push(content_block.clone());
            }
        }

        // -- content_block_delta ---------------------------------------------
        MessageStreamEvent::ContentBlockDelta { delta, index } => {
            if let Some(msg) = current_message {
                let idx = *index;
                if let Some(block) = msg.content.get_mut(idx) {
                    apply_content_delta(block, delta, idx, json_bufs)?;
                }
            }
        }

        // -- content_block_stop (no-op) --------------------------------------
        MessageStreamEvent::ContentBlockStop { .. } => {}

        // -- message_stop ----------------------------------------------------
        MessageStreamEvent::MessageStop => {
            if let Some(msg) = current_message.clone() {
                received_messages.push(msg);
            }
        }

        // -- ping (no-op keep-alive) -----------------------------------------
        MessageStreamEvent::Ping => {}
    }

    Ok(())
}

/// Merge a `MessageDeltaUsage` into the running `Usage` snapshot.
///
/// Maps to: TS `message_delta` branch of `#accumulateMessage()`
fn apply_usage_delta(usage: &mut crate::resources::messages::Usage, delta: &MessageDeltaUsage) {
    usage.output_tokens = delta.output_tokens;

    if let Some(input_tokens) = delta.input_tokens {
        usage.input_tokens = input_tokens;
    }
    if let Some(v) = delta.cache_creation_input_tokens {
        usage.cache_creation_input_tokens = Some(v);
    }
    if let Some(v) = delta.cache_read_input_tokens {
        usage.cache_read_input_tokens = Some(v);
    }
    if let Some(ref v) = delta.server_tool_use {
        usage.server_tool_use = Some(v.clone());
    }
}

/// Apply a content-block-level delta to an existing content block.
///
/// Maps to: TS `content_block_delta` branch of `#accumulateMessage()`
fn apply_content_delta(
    block: &mut ContentBlock,
    delta: &ContentBlockDelta,
    block_index: usize,
    json_bufs: &mut HashMap<usize, String>,
) -> Result<(), ApiError> {
    match delta {
        // -- text_delta ------------------------------------------------------
        ContentBlockDelta::TextDelta { text } => {
            if let ContentBlock::Text { text: existing, .. } = block {
                existing.push_str(text);
            }
        }

        // -- citations_delta -------------------------------------------------
        ContentBlockDelta::CitationsDelta { citation } => {
            if let ContentBlock::Text { citations, .. } = block {
                citations
                    .get_or_insert_with(Vec::new)
                    .push(citation.clone());
            }
        }

        // -- input_json_delta ------------------------------------------------
        ContentBlockDelta::InputJsonDelta { partial_json } => {
            let is_tool_input = matches!(
                block,
                ContentBlock::ToolUse { .. } | ContentBlock::ServerToolUse { .. }
            );
            if is_tool_input {
                let buf = json_bufs.entry(block_index).or_default();
                buf.push_str(partial_json);

                if !buf.is_empty() {
                    let parsed = partial_parse(buf).map_err(|err| {
                        ApiError::Sdk(format!(
                            "Unable to parse tool parameter JSON from model. Please retry your request or adjust your prompt. Error: {err}. JSON: {buf}"
                        ))
                    })?;
                    match block {
                        ContentBlock::ToolUse { input, .. } => {
                            *input = parsed;
                        }
                        ContentBlock::ServerToolUse { input, .. } => {
                            *input = parsed;
                        }
                        _ => {} // unreachable given the guard above
                    }
                }
            }
        }

        // -- thinking_delta --------------------------------------------------
        ContentBlockDelta::ThinkingDelta { thinking } => {
            if let ContentBlock::Thinking {
                thinking: existing, ..
            } = block
            {
                existing.push_str(thinking);
            }
        }

        // -- signature_delta -------------------------------------------------
        ContentBlockDelta::SignatureDelta { signature } => {
            if let ContentBlock::Thinking {
                signature: existing,
                ..
            } = block
            {
                *existing = signature.clone();
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::messages::*;
    use crate::sdk_lib::parser::ParsedContentBlock;
    use futures::stream;
    use futures::StreamExt;

    /// Helper: build a minimal `MessageStreamEvent::MessageStart`.
    fn msg_start_event() -> MessageStreamEvent {
        MessageStreamEvent::MessageStart {
            message: Message {
                id: "msg_test".to_owned(),
                request_id: None,
                content: vec![],
                model: "claude-3-opus-20240229".to_owned(),
                role: "assistant".to_owned(),
                stop_reason: None,
                stop_sequence: None,
                type_name: "message".to_owned(),
                usage: Usage {
                    cache_creation: None,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None,
                    inference_geo: None,
                    input_tokens: 10,
                    output_tokens: 0,
                    server_tool_use: None,
                    service_tier: None,
                },
            },
        }
    }

    fn content_block_start_text(idx: usize) -> MessageStreamEvent {
        MessageStreamEvent::ContentBlockStart {
            index: idx,
            content_block: ContentBlock::Text {
                citations: None,
                text: String::new(),
            },
        }
    }

    fn text_delta_event(idx: usize, text: &str) -> MessageStreamEvent {
        MessageStreamEvent::ContentBlockDelta {
            index: idx,
            delta: ContentBlockDelta::TextDelta {
                text: text.to_owned(),
            },
        }
    }

    fn content_block_stop(idx: usize) -> MessageStreamEvent {
        MessageStreamEvent::ContentBlockStop { index: idx }
    }

    fn msg_delta_event() -> MessageStreamEvent {
        MessageStreamEvent::MessageDelta {
            delta: MessageDelta {
                stop_reason: Some(StopReason::EndTurn),
                stop_sequence: None,
            },
            usage: MessageDeltaUsage {
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                input_tokens: None,
                output_tokens: 15,
                server_tool_use: None,
            },
        }
    }

    fn msg_stop_event() -> MessageStreamEvent {
        MessageStreamEvent::MessageStop
    }

    #[tokio::test]
    async fn test_final_text_single_block() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "Hello")),
            Ok(text_delta_event(0, " world")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let text = ms.final_text().await.unwrap();
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn test_final_text_multiple_blocks() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "First")),
            Ok(content_block_stop(0)),
            Ok(content_block_start_text(1)),
            Ok(text_delta_event(1, "Second")),
            Ok(content_block_stop(1)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let text = ms.final_text().await.unwrap();
        assert_eq!(text, "First Second");
    }

    #[tokio::test]
    async fn test_final_message_stop_reason() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "Hi")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let msg = ms.final_message().await.unwrap();
        assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
        assert_eq!(msg.usage.output_tokens, 15);
    }

    #[tokio::test]
    async fn test_final_parsed_message_parses_streamed_text_blocks() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Output {
            answer: i64,
        }

        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "{\"answer\"")),
            Ok(text_delta_event(0, ":42}")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let parsed = MessageStream::new(sse)
            .final_parsed_message::<Output>()
            .await
            .unwrap();
        assert_eq!(parsed.parsed_output.unwrap(), Output { answer: 42 });
        match &parsed.content[0] {
            ParsedContentBlock::Text { parsed_output, .. } => {
                assert_eq!(parsed_output.as_ref().unwrap().answer, 42)
            }
            other => panic!("expected parsed text block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_abort_stops_stream() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "Hi")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let mut ms = MessageStream::new(sse);

        // Consume the first event then abort.
        let first = ms.next().await;
        assert!(first.is_some());
        ms.abort();

        // After abort, stream should end immediately.
        let next = ms.next().await;
        assert!(next.is_none());
        assert!(ms.aborted());
    }

    #[tokio::test]
    async fn test_empty_stream_returns_error() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![];
        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let result = ms.final_message().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_thinking_block_accumulation() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: String::new(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::ThinkingDelta {
                    thinking: "Let me think".to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::ThinkingDelta {
                    thinking: " about this.".to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::SignatureDelta {
                    signature: "sig123".to_owned(),
                },
            }),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let msg = ms.final_message().await;
        assert!(msg.is_ok());
        let msg = msg.as_ref();
        match msg.map(|m| &m.content[0]) {
            Ok(ContentBlock::Thinking {
                thinking,
                signature,
            }) => {
                assert_eq!(thinking, "Let me think about this.");
                assert_eq!(signature, "sig123");
            }
            other => panic!("expected Thinking block, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_tool_use_input_json_delta() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::ToolUse {
                    id: "tool_1".to_owned(),
                    input: serde_json::Value::Object(serde_json::Map::new()),
                    name: "get_weather".to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#"{"city": "San"#.to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#" Francisco"}"#.to_owned(),
                },
            }),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let msg = ms.final_message().await;
        assert!(msg.is_ok());
        let msg = msg.as_ref();
        match msg.map(|m| &m.content[0]) {
            Ok(ContentBlock::ToolUse { input, name, .. }) => {
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "San Francisco");
            }
            other => panic!("expected ToolUse block, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_ping_events_are_yielded_but_ignored() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(MessageStreamEvent::Ping),
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::Ping),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "ok")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let text = ms.final_text().await.unwrap();
        assert_eq!(text, "ok");
    }

    #[tokio::test]
    async fn test_malformed_tool_use_input_json_delta_errors() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::ToolUse {
                    id: "toolu_bad".to_owned(),
                    input: serde_json::json!({}),
                    name: "test_tool".to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#"{"foo": "bar", "baz": "#.to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#""qux": "quux"}"#.to_owned(),
                },
            }),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let err = MessageStream::new(sse).final_message().await.unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("Unable to parse tool parameter JSON from model"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains(r#"{"foo": "bar", "baz": "qux": "quux"}"#),
            "unexpected error: {message}"
        );
    }

    #[tokio::test]
    async fn test_stream_error_propagated() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> =
            vec![Ok(msg_start_event()), Err(ApiError::Sdk("boom".to_owned()))];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let result = ms.final_message().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_current_message_during_stream() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "partial")),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let mut ms = MessageStream::new(sse);

        // Before any polling, no current message.
        assert!(ms.current_message().is_none());

        // After consuming message_start, snapshot should be set.
        let _ = ms.next().await;
        assert!(ms.current_message().is_some());
        assert_eq!(
            ms.current_message().map(|m| m.id.as_str()),
            Some("msg_test")
        );

        // After a content_block_start + text_delta, text should accumulate.
        let _ = ms.next().await; // content_block_start
        let _ = ms.next().await; // text_delta
        match ms.current_message().and_then(|m| m.content.first()) {
            Some(ContentBlock::Text { text, .. }) => {
                assert_eq!(text, "partial");
            }
            other => panic!("expected Text block, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_received_messages_after_stop() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "done")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let mut ms = MessageStream::new(sse);

        // Drain the stream.
        while let Some(r) = ms.next().await {
            assert!(r.is_ok());
        }

        assert_eq!(ms.received_messages().len(), 1);
        assert_eq!(ms.received_messages()[0].id, "msg_test");
    }

    #[tokio::test]
    async fn test_no_text_blocks_returns_error() {
        // A message that only has a thinking block, no text.
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::Thinking {
                    thinking: String::new(),
                    signature: String::new(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::ThinkingDelta {
                    thinking: "hmm".to_owned(),
                },
            }),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let result = ms.final_text().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_tool_use_input_json_delta() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(MessageStreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::ServerToolUse {
                    id: "stu_1".to_owned(),
                    input: serde_json::Value::Object(serde_json::Map::new()),
                    name: "web_search".to_owned(),
                },
            }),
            Ok(MessageStreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentBlockDelta::InputJsonDelta {
                    partial_json: r#"{"query": "rust"}"#.to_owned(),
                },
            }),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let msg = ms.final_message().await;
        assert!(msg.is_ok());
        match msg.as_ref().map(|m| &m.content[0]) {
            Ok(ContentBlock::ServerToolUse { input, name, .. }) => {
                assert_eq!(name, "web_search");
                assert_eq!(input["query"], "rust");
            }
            other => panic!("expected ServerToolUse block, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_usage_delta_fields() {
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "x")),
            Ok(content_block_stop(0)),
            Ok(MessageStreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                    stop_sequence: None,
                },
                usage: MessageDeltaUsage {
                    cache_creation_input_tokens: Some(100),
                    cache_read_input_tokens: Some(200),
                    input_tokens: Some(50),
                    output_tokens: 42,
                    server_tool_use: Some(ServerToolUsage {
                        web_search_requests: 3,
                    }),
                },
            }),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let msg = ms.final_message().await.unwrap();
        let u = &msg.usage;
        assert_eq!(u.output_tokens, 42);
        assert_eq!(u.input_tokens, 50);
        assert_eq!(u.cache_creation_input_tokens, Some(100));
        assert_eq!(u.cache_read_input_tokens, Some(200));
        assert!(u.server_tool_use.is_some());
    }

    // -----------------------------------------------------------------------
    // Fixture-based tests (ported from TS MessageStream.test.ts)
    // -----------------------------------------------------------------------

    /// Parse an SSE fixture file into a Vec of MessageStreamEvent.
    ///
    /// The fixture uses the standard SSE format:
    ///   event: <event_type>
    ///   data: <json>
    ///   <blank line>
    ///
    /// For "ping" events the data is ignored and `MessageStreamEvent::Ping`
    /// is emitted.  For all other message events the data is deserialized
    /// via serde.
    fn parse_fixture(content: &str) -> Vec<MessageStreamEvent> {
        let mut events = Vec::new();
        let mut current_event: Option<String> = None;
        let mut current_data: Vec<String> = Vec::new();

        for line in content.lines() {
            if line.is_empty() {
                // End of an SSE frame -- emit if we have accumulated data.
                if let Some(ref event_name) = current_event {
                    let data = current_data.join("\n");
                    let evt = match event_name.as_str() {
                        "ping" => MessageStreamEvent::Ping,
                        _ => {
                            serde_json::from_str::<MessageStreamEvent>(&data).unwrap_or_else(|e| {
                                panic!(
                                    "failed to parse fixture event '{}': {}\ndata: {}",
                                    event_name, e, data
                                )
                            })
                        }
                    };
                    events.push(evt);
                }
                current_event = None;
                current_data.clear();
                continue;
            }
            if let Some(stripped) = line.strip_prefix("event: ") {
                current_event = Some(stripped.to_owned());
            } else if let Some(stripped) = line.strip_prefix("data: ") {
                current_data.push(stripped.to_owned());
            }
        }

        // Handle trailing frame without a final blank line.
        if let Some(ref event_name) = current_event {
            if !current_data.is_empty() {
                let data = current_data.join("\n");
                let evt = match event_name.as_str() {
                    "ping" => MessageStreamEvent::Ping,
                    _ => serde_json::from_str::<MessageStreamEvent>(&data).unwrap_or_else(|e| {
                        panic!(
                            "failed to parse trailing fixture event '{}': {}\ndata: {}",
                            event_name, e, data
                        )
                    }),
                };
                events.push(evt);
            }
        }

        events
    }

    /// Helper: wrap parsed fixture events into a MessageStream.
    fn fixture_stream(fixture_events: Vec<MessageStreamEvent>) -> MessageStream {
        let items: Vec<Result<MessageStreamEvent, ApiError>> =
            fixture_events.into_iter().map(Ok).collect();
        let sse: SseStream = Box::pin(stream::iter(items));
        MessageStream::new(sse)
    }

    /// Helper: return the event type name as a &str for assertions.
    fn event_type_name(event: &MessageStreamEvent) -> &'static str {
        match event {
            MessageStreamEvent::MessageStart { .. } => "message_start",
            MessageStreamEvent::MessageDelta { .. } => "message_delta",
            MessageStreamEvent::MessageStop => "message_stop",
            MessageStreamEvent::ContentBlockStart { .. } => "content_block_start",
            MessageStreamEvent::ContentBlockDelta { .. } => "content_block_delta",
            MessageStreamEvent::ContentBlockStop { .. } => "content_block_stop",
            MessageStreamEvent::Ping => "ping",
        }
    }

    // -- basic_response.txt fixture -----------------------------------------

    #[tokio::test]
    async fn test_basic_fixture() {
        let fixture = include_str!("../../tests/fixtures/basic_response.txt");
        let fixture_events = parse_fixture(fixture);
        let mut ms = fixture_stream(fixture_events);

        let mut events = Vec::new();
        while let Some(result) = ms.next().await {
            events.push(result.expect("fixture event should not error"));
        }

        let msg = ms
            .received_messages()
            .last()
            .expect("should have a final message");

        // Verify event type sequence (excluding ping).
        let event_types: Vec<&str> = events
            .iter()
            .filter(|e| !matches!(e, MessageStreamEvent::Ping))
            .map(|e| event_type_name(e))
            .collect();
        assert_eq!(
            event_types,
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );

        // Verify assembled message fields.
        assert_eq!(msg.id, "msg_4QpJur2dWWDjF6C758FbBw5vm12BaVipnK");
        assert_eq!(msg.model, "claude-opus-4-20250514");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
        assert!(msg.stop_sequence.is_none());
        assert_eq!(msg.type_name, "message");

        // Verify content.
        assert_eq!(msg.content.len(), 1);
        match &msg.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "Hello there!");
            }
            other => panic!("expected Text block, got {:?}", other),
        }

        // Verify usage.
        assert_eq!(msg.usage.input_tokens, 11);
        assert_eq!(msg.usage.output_tokens, 6);
    }

    #[tokio::test]
    async fn test_basic_fixture_final_text() {
        let fixture = include_str!("../../tests/fixtures/basic_response.txt");
        let fixture_events = parse_fixture(fixture);
        let ms = fixture_stream(fixture_events);

        let text = ms.final_text().await.unwrap();
        assert_eq!(text, "Hello there!");
    }

    // -- tool_use_response.txt fixture --------------------------------------

    #[tokio::test]
    async fn test_tool_use_fixture() {
        let fixture = include_str!("../../tests/fixtures/tool_use_response.txt");
        let fixture_events = parse_fixture(fixture);
        let mut ms = fixture_stream(fixture_events);

        let mut events = Vec::new();
        while let Some(result) = ms.next().await {
            events.push(result.expect("fixture event should not error"));
        }

        let msg = ms
            .received_messages()
            .last()
            .expect("should have a final message");

        // Verify event type sequence (excluding ping).
        let event_types: Vec<&str> = events
            .iter()
            .filter(|e| !matches!(e, MessageStreamEvent::Ping))
            .map(|e| event_type_name(e))
            .collect();
        assert_eq!(
            event_types,
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_delta",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );

        // Verify assembled message fields.
        assert_eq!(msg.id, "msg_019Q1hrJbZG26Fb9BQhrkHEr");
        assert_eq!(msg.model, "claude-sonnet-4-20250514");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.stop_reason, Some(StopReason::ToolUse));
        assert!(msg.stop_sequence.is_none());

        // Verify content blocks.
        assert_eq!(msg.content.len(), 2);
        match &msg.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "I'll check the current weather in Paris for you.");
            }
            other => panic!("expected Text block at index 0, got {:?}", other),
        }
        match &msg.content[1] {
            ContentBlock::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_01NRLabsLyVHZPKxbKvkfSMn");
                assert_eq!(name, "get_weather");
                assert_eq!(input["location"], "Paris");
            }
            other => panic!("expected ToolUse block at index 1, got {:?}", other),
        }

        // Verify usage.
        assert_eq!(msg.usage.input_tokens, 377);
        assert_eq!(msg.usage.output_tokens, 65);
    }

    #[tokio::test]
    async fn test_tool_use_fixture_final_text() {
        let fixture = include_str!("../../tests/fixtures/tool_use_response.txt");
        let fixture_events = parse_fixture(fixture);
        let ms = fixture_stream(fixture_events);

        let text = ms.final_text().await.unwrap();
        assert_eq!(text, "I'll check the current weather in Paris for you.");
    }

    // -- abort on break (TS: "aborts on break") -----------------------------

    #[tokio::test]
    async fn test_abort_on_break() {
        // Simulates breaking out of a for-await loop mid-stream.
        // After breaking, the stream should be abortable and yield no more events.
        let fixture = include_str!("../../tests/fixtures/basic_response.txt");
        let fixture_events = parse_fixture(fixture);
        let items: Vec<Result<MessageStreamEvent, ApiError>> =
            fixture_events.into_iter().map(Ok).collect();
        let sse: SseStream = Box::pin(stream::iter(items));
        let mut ms = MessageStream::new(sse);

        // Iterate until we find a text_delta containing "He", then break.
        let mut found_break = false;
        while let Some(result) = ms.next().await {
            let event = result.expect("should not error");
            if let MessageStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { ref text },
                ..
            } = event
            {
                if text.contains("He") {
                    found_break = true;
                    break;
                }
            }
        }
        assert!(found_break, "should have found a text_delta with 'He'");

        // After the break, abort the stream.
        ms.abort();
        assert!(ms.aborted());

        // No more events should be yielded.
        let next = ms.next().await;
        assert!(next.is_none());
    }

    // -- network error propagated -------------------------------------------

    #[tokio::test]
    async fn test_network_error_propagated() {
        // Simulates a connection error mid-stream. The error should propagate
        // through final_message().
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "partial")),
            Err(ApiError::Connection {
                message: "mock network error".to_owned(),
                cause: None,
            }),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let ms = MessageStream::new(sse);
        let result = ms.final_message().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ApiError::Connection { message, .. } => {
                assert!(
                    message.contains("mock network error"),
                    "expected 'mock network error' in: {}",
                    message
                );
            }
            other => panic!("expected Connection error, got {:?}", other),
        }
    }

    // -- event sequence correct (TS: verifies event types match expected) ----

    #[tokio::test]
    async fn test_event_sequence_correct() {
        // Verify a standard text-only stream yields events in the correct order.
        let events: Vec<Result<MessageStreamEvent, ApiError>> = vec![
            Ok(msg_start_event()),
            Ok(content_block_start_text(0)),
            Ok(text_delta_event(0, "Hello")),
            Ok(text_delta_event(0, " world")),
            Ok(content_block_stop(0)),
            Ok(msg_delta_event()),
            Ok(msg_stop_event()),
        ];

        let sse: SseStream = Box::pin(stream::iter(events));
        let mut ms = MessageStream::new(sse);

        let mut event_types = Vec::new();
        while let Some(result) = ms.next().await {
            let event = result.expect("should not error");
            event_types.push(event_type_name(&event));
        }

        assert_eq!(
            event_types,
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );

        // Verify accumulated message is correct after full consumption.
        assert_eq!(ms.received_messages().len(), 1);
        let msg = &ms.received_messages()[0];
        assert_eq!(msg.stop_reason, Some(StopReason::EndTurn));
        match &msg.content[0] {
            ContentBlock::Text { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected Text block, got {:?}", other),
        }
    }
}

// Maps to: TS lib/BetaMessageStream.ts
//
// High-level wrapper around the beta SSE event stream. It mirrors
// `MessageStream` but accumulates beta-specific message/content-block types
// such as context management, MCP tool use, and compaction deltas.

use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::Stream;
use pin_project_lite::pin_project;

use crate::core::error::ApiError;
use crate::resources::beta::messages::{
    BetaContentBlock, BetaContentBlockDelta, BetaMessage, BetaMessageDeltaUsage,
    BetaMessageStreamEvent, BetaServerToolInput, BetaServerToolName, BetaToolCaller, BetaToolInput,
};
use crate::sdk_lib::beta_parser::{ParsedBetaMessage, parse_beta_message};
use crate::vendor::partial_json_parser::partial_parse;

/// Content block variants whose `input` is incrementally reconstructed from
/// beta `input_json_delta` stream chunks.
///
/// Maps to TS `export type TracksToolInput = BetaToolUseBlock |
/// BetaServerToolUseBlock | BetaMCPToolUseBlock` in `lib/BetaMessageStream.ts`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum TracksToolInput {
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        input: BetaToolInput,
        name: String,
    },

    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<BetaToolCaller>,
        id: String,
        input: BetaServerToolInput,
        name: BetaServerToolName,
    },

    #[serde(rename = "mcp_tool_use")]
    McpToolUse {
        id: String,
        input: BetaToolInput,
        name: String,
        server_name: String,
    },
}

impl TracksToolInput {
    /// Borrow the tool-use id, matching the common TS union property.
    pub fn id(&self) -> &str {
        match self {
            TracksToolInput::ToolUse { id, .. }
            | TracksToolInput::ServerToolUse { id, .. }
            | TracksToolInput::McpToolUse { id, .. } => id,
        }
    }

    /// Borrow the tool name as JSON text for the beta server-tool enum/string
    /// union cases.
    pub fn name_json(&self) -> serde_json::Value {
        match self {
            TracksToolInput::ToolUse { name, .. } | TracksToolInput::McpToolUse { name, .. } => {
                serde_json::Value::String(name.clone())
            }
            TracksToolInput::ServerToolUse { name, .. } => {
                serde_json::to_value(name).unwrap_or(serde_json::Value::Null)
            }
        }
    }
}

/// Rust equivalent of TS beta `tracksToolInput(content)` guard.
pub fn tracks_tool_input(content: &BetaContentBlock) -> Option<TracksToolInput> {
    match content {
        BetaContentBlock::ToolUse {
            id, input, name, ..
        } => Some(TracksToolInput::ToolUse {
            id: id.clone(),
            input: input.clone(),
            name: name.clone(),
        }),
        BetaContentBlock::ServerToolUse {
            caller,
            id,
            input,
            name,
        } => Some(TracksToolInput::ServerToolUse {
            caller: caller.clone(),
            id: id.clone(),
            input: input.clone(),
            name: name.clone(),
        }),
        BetaContentBlock::McpToolUse {
            id,
            input,
            name,
            server_name,
        } => Some(TracksToolInput::McpToolUse {
            id: id.clone(),
            input: input.clone(),
            name: name.clone(),
            server_name: server_name.clone(),
        }),
        _ => None,
    }
}

impl<'a> TryFrom<&'a BetaContentBlock> for TracksToolInput {
    type Error = ApiError;

    fn try_from(value: &'a BetaContentBlock) -> Result<Self, Self::Error> {
        tracks_tool_input(value).ok_or_else(|| {
            ApiError::Sdk(
                "beta content block does not contain stream-tracked tool input".to_owned(),
            )
        })
    }
}

/// Rust stream-item counterpart to TS beta `MessageStreamEvents`.
///
/// The TS SDK exposes callback signatures; Rust exposes these lifecycle events
/// through its `Stream` item type.
pub type MessageStreamEvents = BetaMessageStreamEvent;

/// Explicitly beta-prefixed compatibility alias for callers importing both
/// stable and beta stream modules.
pub type BetaMessageStreamEvents = BetaMessageStreamEvent;

/// A boxed, pinned, `Send`-able stream of beta SSE events.
pub type BetaSseStream =
    Pin<Box<dyn Stream<Item = Result<BetaMessageStreamEvent, ApiError>> + Send + 'static>>;

pin_project! {
    /// Wraps a beta SSE event stream and accumulates events into a
    /// [`BetaMessage`] snapshot.
    ///
    /// Maps to: TS `BetaMessageStream`.
    pub struct BetaMessageStream {
        #[pin]
        inner: BetaSseStream,
        current_message: Option<BetaMessage>,
        received_messages: Vec<BetaMessage>,
        aborted: bool,
        json_bufs: HashMap<usize, String>,
    }
}

impl BetaMessageStream {
    /// Create a new `BetaMessageStream` from a lower-level SSE stream.
    pub fn new(stream: BetaSseStream) -> Self {
        Self {
            inner: stream,
            current_message: None,
            received_messages: Vec::new(),
            aborted: false,
            json_bufs: HashMap::new(),
        }
    }

    /// Reconstruct a high-level beta message stream from newline-delimited
    /// JSON stream-event bytes.
    ///
    /// Maps to: TS `BetaMessageStream.fromReadableStream()`.
    pub fn from_readable_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        let events =
            crate::core::streaming::from_readable_stream::<BetaMessageStreamEvent, _, _>(stream);
        Self::new(Box::pin(events))
    }

    /// TS-style camelCase alias for [`BetaMessageStream::from_readable_stream`].
    #[allow(non_snake_case)]
    pub fn fromReadableStream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<bytes::Bytes, E>> + Send + 'static,
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::from_readable_stream(stream)
    }

    /// Returns the current in-progress beta message snapshot, if any.
    pub fn current_message(&self) -> Option<&BetaMessage> {
        self.current_message.as_ref()
    }

    /// Returns all fully-received beta messages so far.
    pub fn received_messages(&self) -> &[BetaMessage] {
        &self.received_messages
    }

    /// Returns `true` if [`abort`](Self::abort) has been called.
    pub fn aborted(&self) -> bool {
        self.aborted
    }

    /// Signal the stream to stop producing events.
    pub fn abort(&mut self) {
        self.aborted = true;
    }

    /// Consumes the stream and drains it to completion.
    ///
    /// Maps to: TS `async done()`.
    pub async fn done(mut self) -> Result<(), ApiError> {
        use futures::StreamExt;
        while let Some(result) = self.next().await {
            result?;
        }
        Ok(())
    }

    /// Consumes the stream and serializes each raw beta event as
    /// newline-delimited JSON bytes.
    ///
    /// Maps to: TS `BetaMessageStream.toReadableStream()`.
    pub fn to_readable_stream(self) -> impl Stream<Item = Result<bytes::Bytes, ApiError>> {
        crate::core::streaming::to_readable_stream(self)
    }

    /// TS-style camelCase alias for [`BetaMessageStream::to_readable_stream`].
    #[allow(non_snake_case)]
    pub fn toReadableStream(self) -> impl Stream<Item = Result<bytes::Bytes, ApiError>> {
        self.to_readable_stream()
    }

    /// Consumes the stream and returns the final beta assistant message.
    ///
    /// Maps to: TS `async finalMessage()`.
    pub async fn final_message(mut self) -> Result<BetaMessage, ApiError> {
        use futures::StreamExt;
        while let Some(result) = self.next().await {
            result?;
        }
        self.received_messages.pop().ok_or_else(|| {
            ApiError::Sdk(
                "stream ended without producing a BetaMessage with role=assistant".to_owned(),
            )
        })
    }

    /// TS-style camelCase alias for [`BetaMessageStream::final_message`].
    #[allow(non_snake_case)]
    pub async fn finalMessage(self) -> Result<BetaMessage, ApiError> {
        self.final_message().await
    }

    /// Consumes the stream and returns the final beta message parsed as
    /// structured output.
    ///
    /// TypeScript's `BetaMessageStream<ParsedT>.finalMessage()` returns a
    /// parsed message when the stream was created with an auto-parseable beta
    /// output format. Rust streams are not generic, so callers opt in
    /// explicitly with this helper and the target serde type `T`.
    pub async fn final_parsed_message<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<ParsedBetaMessage<T>, ApiError> {
        let message = self.final_message().await?;
        parse_beta_message(&message)
    }

    /// TS-style camelCase alias for [`BetaMessageStream::final_parsed_message`].
    #[allow(non_snake_case)]
    pub async fn finalParsedMessage<T: serde::de::DeserializeOwned>(
        self,
    ) -> Result<ParsedBetaMessage<T>, ApiError> {
        self.final_parsed_message().await
    }

    /// Consumes the stream and returns concatenated text from the final beta
    /// message.
    ///
    /// Maps to: TS `async finalText()`.
    pub async fn final_text(self) -> Result<String, ApiError> {
        let message = self.final_message().await?;
        let texts: Vec<&str> = message
            .content
            .iter()
            .filter_map(|block| match block {
                BetaContentBlock::Text { text, .. } => Some(text.as_str()),
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

    /// TS-style camelCase alias for [`BetaMessageStream::final_text`].
    #[allow(non_snake_case)]
    pub async fn finalText(self) -> Result<String, ApiError> {
        self.final_text().await
    }
}

impl Stream for BetaMessageStream {
    type Item = Result<BetaMessageStreamEvent, ApiError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        if *this.aborted {
            return Poll::Ready(None);
        }

        match this.inner.poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(Some(Ok(event))) => match accumulate_into(
                this.current_message,
                this.received_messages,
                this.json_bufs,
                &event,
            ) {
                Ok(()) => Poll::Ready(Some(Ok(event))),
                Err(err) => Poll::Ready(Some(Err(err))),
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

fn accumulate_into(
    current_message: &mut Option<BetaMessage>,
    received_messages: &mut Vec<BetaMessage>,
    json_bufs: &mut HashMap<usize, String>,
    event: &BetaMessageStreamEvent,
) -> Result<(), ApiError> {
    match event {
        BetaMessageStreamEvent::MessageStart { message } => {
            json_bufs.clear();
            *current_message = Some(message.as_ref().clone());
        }
        BetaMessageStreamEvent::MessageDelta {
            delta,
            usage,
            context_management,
        } => {
            if let Some(msg) = current_message {
                msg.container = delta.container.clone();
                msg.stop_reason = delta.stop_reason.clone();
                msg.stop_sequence = delta.stop_sequence.clone();
                msg.context_management = context_management.clone();
                apply_usage_delta(&mut msg.usage, usage);
            }
        }
        BetaMessageStreamEvent::MessageStop => {
            if let Some(msg) = current_message.clone() {
                received_messages.push(msg);
            }
        }
        BetaMessageStreamEvent::ContentBlockStart {
            content_block,
            index: _,
        } => {
            if let Some(msg) = current_message {
                msg.content.push(content_block.clone());
            }
        }
        BetaMessageStreamEvent::ContentBlockDelta { delta, index } => {
            if let Some(msg) = current_message {
                let idx = *index;
                if let Some(block) = msg.content.get_mut(idx) {
                    apply_content_delta(block, delta, idx, json_bufs)?;
                }
            }
        }
        BetaMessageStreamEvent::ContentBlockStop { .. } => {}
        BetaMessageStreamEvent::Ping => {}
    }

    Ok(())
}

fn apply_usage_delta(
    usage: &mut crate::resources::beta::messages::BetaUsage,
    delta: &BetaMessageDeltaUsage,
) {
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
    if let Some(ref v) = delta.iterations {
        usage.iterations = Some(v.clone());
    }
}

fn apply_content_delta(
    block: &mut BetaContentBlock,
    delta: &BetaContentBlockDelta,
    block_index: usize,
    json_bufs: &mut HashMap<usize, String>,
) -> Result<(), ApiError> {
    match delta {
        BetaContentBlockDelta::TextDelta { text } => {
            if let BetaContentBlock::Text { text: existing, .. } = block {
                existing.push_str(text);
            }
        }
        BetaContentBlockDelta::CitationsDelta { citation } => {
            if let BetaContentBlock::Text { citations, .. } = block {
                citations
                    .get_or_insert_with(Vec::new)
                    .push(citation.clone());
            }
        }
        BetaContentBlockDelta::InputJsonDelta { partial_json } => {
            let tracks_tool_input = matches!(
                block,
                BetaContentBlock::ToolUse { .. }
                    | BetaContentBlock::ServerToolUse { .. }
                    | BetaContentBlock::McpToolUse { .. }
            );
            if tracks_tool_input {
                let buf = json_bufs.entry(block_index).or_default();
                buf.push_str(partial_json);

                if !buf.is_empty() {
                    let parsed = partial_parse(buf).map_err(|err| {
                        ApiError::Sdk(format!(
                            "Unable to parse tool parameter JSON from model. Please retry your request or adjust your prompt. Error: {err}. JSON: {buf}"
                        ))
                    })?;
                    match block {
                        BetaContentBlock::ToolUse { input, .. }
                        | BetaContentBlock::McpToolUse { input, .. } => {
                            *input = parsed;
                        }
                        BetaContentBlock::ServerToolUse { input, .. } => {
                            if let serde_json::Value::Object(map) = parsed {
                                *input = map.into_iter().collect();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        BetaContentBlockDelta::ThinkingDelta { thinking } => {
            if let BetaContentBlock::Thinking {
                thinking: existing, ..
            } = block
            {
                existing.push_str(thinking);
            }
        }
        BetaContentBlockDelta::SignatureDelta { signature } => {
            if let BetaContentBlock::Thinking {
                signature: existing,
                ..
            } = block
            {
                *existing = signature.clone();
            }
        }
        BetaContentBlockDelta::CompactionDelta { content } => {
            if let (BetaContentBlock::Compaction { content: existing }, Some(chunk)) =
                (block, content)
            {
                let mut next = existing.take().unwrap_or_default();
                next.push_str(chunk);
                *existing = Some(next);
            }
        }
    }

    Ok(())
}

// Mirrors TS SDK tests/api-resources/MessageStream.test.ts fixture cases.

use anthropic_sdk::ApiError;
use anthropic_sdk::resources::messages::{
    ContentBlock, Message, MessageStreamEvent, ServiceTier, StopReason,
};
use anthropic_sdk::sdk_lib::message_stream::{MessageStream, SseStream};
use futures::{StreamExt, stream};

const EXPECTED_BASIC_EVENT_TYPES: &[&str] = &[
    "message_start",
    "content_block_start",
    "content_block_delta",
    "content_block_delta",
    "content_block_delta",
    "content_block_stop",
    "message_delta",
    "message_stop",
];

const EXPECTED_TOOL_USE_EVENT_TYPES: &[&str] = &[
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
];

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

fn parse_sse_fixture(content: &str) -> Vec<MessageStreamEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            if let Some(ref event_name) = current_event {
                let data = current_data.join("\n");
                let event = match event_name.as_str() {
                    "ping" => MessageStreamEvent::Ping,
                    _ => serde_json::from_str::<MessageStreamEvent>(&data).unwrap_or_else(|e| {
                        panic!("failed to parse fixture event '{event_name}': {e}; data: {data}")
                    }),
                };
                events.push(event);
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

    if let Some(ref event_name) = current_event {
        if !current_data.is_empty() {
            let data = current_data.join("\n");
            let event = match event_name.as_str() {
                "ping" => MessageStreamEvent::Ping,
                _ => serde_json::from_str::<MessageStreamEvent>(&data).unwrap_or_else(|e| {
                    panic!(
                        "failed to parse trailing fixture event '{event_name}': {e}; data: {data}"
                    )
                }),
            };
            events.push(event);
        }
    }

    events
}

fn fixture_stream(fixture: &str) -> MessageStream {
    let items: Vec<Result<MessageStreamEvent, ApiError>> =
        parse_sse_fixture(fixture).into_iter().map(Ok).collect();
    let sse: SseStream = Box::pin(stream::iter(items));
    MessageStream::new(sse)
}

async fn collect_events_and_final_message(
    mut stream: MessageStream,
) -> (Vec<MessageStreamEvent>, Message) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("fixture event should be valid"));
    }
    let final_message = stream
        .received_messages()
        .last()
        .expect("fixture should produce a final message")
        .clone();
    (events, final_message)
}

fn non_ping_event_types(events: &[MessageStreamEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter(|event| !matches!(event, MessageStreamEvent::Ping))
        .map(event_type_name)
        .collect()
}

fn assert_basic_response(events: &[MessageStreamEvent], message: &Message) {
    assert_eq!(non_ping_event_types(events), EXPECTED_BASIC_EVENT_TYPES);
    assert_eq!(message.id, "msg_4QpJur2dWWDjF6C758FbBw5vm12BaVipnK");
    assert_eq!(message.model, "claude-opus-4-20250514");
    assert_eq!(message.role, "assistant");
    assert_eq!(message.stop_reason, Some(StopReason::EndTurn));
    assert!(message.stop_sequence.is_none());
    assert_eq!(message.type_name, "message");
    assert_eq!(message.usage.input_tokens, 11);
    assert_eq!(message.usage.output_tokens, 6);
    assert_eq!(message.content.len(), 1);
    match &message.content[0] {
        ContentBlock::Text { text, .. } => assert_eq!(text, "Hello there!"),
        other => panic!("expected text block, got {other:?}"),
    }
}

fn assert_tool_use_response(events: &[MessageStreamEvent], message: &Message) {
    assert_eq!(non_ping_event_types(events), EXPECTED_TOOL_USE_EVENT_TYPES);
    assert_eq!(message.id, "msg_019Q1hrJbZG26Fb9BQhrkHEr");
    assert_eq!(message.model, "claude-sonnet-4-20250514");
    assert_eq!(message.role, "assistant");
    assert_eq!(message.stop_reason, Some(StopReason::ToolUse));
    assert!(message.stop_sequence.is_none());
    assert_eq!(message.content.len(), 2);
    match &message.content[0] {
        ContentBlock::Text { text, .. } => {
            assert_eq!(text, "I'll check the current weather in Paris for you.")
        }
        other => panic!("expected text block at index 0, got {other:?}"),
    }
    match &message.content[1] {
        ContentBlock::ToolUse { id, input, name } => {
            assert_eq!(id, "toolu_01NRLabsLyVHZPKxbKvkfSMn");
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "Paris");
        }
        other => panic!("expected tool_use block at index 1, got {other:?}"),
    }
    assert_eq!(message.usage.input_tokens, 377);
    assert_eq!(message.usage.output_tokens, 65);
    assert!(matches!(
        message.usage.service_tier,
        Some(ServiceTier::Standard)
    ));
}

#[tokio::test]
async fn message_stream_handles_basic_response_fixture() {
    let fixture = include_str!("../fixtures/basic_response.txt");
    let (events, final_message) = collect_events_and_final_message(fixture_stream(fixture)).await;
    let final_text = fixture_stream(fixture).final_text().await.unwrap();

    assert_basic_response(&events, &final_message);
    assert_eq!(final_text, "Hello there!");
}

#[tokio::test]
async fn message_stream_ts_style_terminal_aliases_and_readable_stream_work() {
    let fixture = include_str!("../fixtures/basic_response.txt");

    let final_message = fixture_stream(fixture).finalMessage().await.unwrap();
    assert_eq!(final_message.id, "msg_4QpJur2dWWDjF6C758FbBw5vm12BaVipnK");
    assert_eq!(
        fixture_stream(fixture).finalText().await.unwrap(),
        "Hello there!"
    );
    fixture_stream(fixture).done().await.unwrap();

    let mut readable = fixture_stream(fixture).toReadableStream();
    let first_chunk = readable.next().await.unwrap().unwrap();
    let first_line = String::from_utf8(first_chunk.to_vec()).unwrap();
    let event: serde_json::Value = serde_json::from_str(first_line.trim_end()).unwrap();
    assert_eq!(event["type"], "message_start");

    let roundtrip_text =
        MessageStream::fromReadableStream(fixture_stream(fixture).toReadableStream())
            .finalText()
            .await
            .unwrap();
    assert_eq!(roundtrip_text, "Hello there!");
}

#[tokio::test]
async fn message_stream_handles_tool_use_response_fixture() {
    let fixture = include_str!("../fixtures/tool_use_response.txt");
    let (events, final_message) = collect_events_and_final_message(fixture_stream(fixture)).await;
    let final_text = fixture_stream(fixture).final_text().await.unwrap();

    assert_tool_use_response(&events, &final_message);
    assert_eq!(
        final_text,
        "I'll check the current weather in Paris for you."
    );
}

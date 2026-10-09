// Integration/parity tests for lib/BetaMessageStream.
//
// Mirrors TS SDK beta stream accumulation behavior for beta-only fields and
// content block deltas.

use anthropic_sdk::ApiError;
use anthropic_sdk::resources::beta::messages::{
    BetaContentBlock, BetaContentBlockDelta, BetaContextManagementAppliedEdit,
    BetaContextManagementResponse, BetaIterationUsage, BetaMessage, BetaMessageDelta,
    BetaMessageDeltaUsage, BetaMessageStreamEvent, BetaStopReason, BetaUsage,
};
use anthropic_sdk::sdk_lib::beta_message_stream::{BetaMessageStream, BetaSseStream};
use anthropic_sdk::sdk_lib::beta_parser::ParsedBetaContentBlock;
use futures::{StreamExt, stream};

fn message_start() -> BetaMessageStreamEvent {
    BetaMessageStreamEvent::MessageStart {
        message: Box::new(BetaMessage {
            id: "msg_beta_stream".to_owned(),
            model: "claude-3-5-sonnet-latest".to_owned(),
            usage: BetaUsage {
                input_tokens: 10,
                ..Default::default()
            },
            ..Default::default()
        }),
    }
}

fn event_type_name(event: &BetaMessageStreamEvent) -> &'static str {
    match event {
        BetaMessageStreamEvent::MessageStart { .. } => "message_start",
        BetaMessageStreamEvent::MessageDelta { .. } => "message_delta",
        BetaMessageStreamEvent::MessageStop => "message_stop",
        BetaMessageStreamEvent::ContentBlockStart { .. } => "content_block_start",
        BetaMessageStreamEvent::ContentBlockDelta { .. } => "content_block_delta",
        BetaMessageStreamEvent::ContentBlockStop { .. } => "content_block_stop",
        BetaMessageStreamEvent::Ping => "ping",
    }
}

fn parse_sse_fixture(content: &str) -> Vec<BetaMessageStreamEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            if let Some(ref event_name) = current_event {
                let data = current_data.join("\n");
                let event = match event_name.as_str() {
                    "ping" => BetaMessageStreamEvent::Ping,
                    _ => serde_json::from_str::<BetaMessageStreamEvent>(&data).unwrap_or_else(|e| {
                        panic!("failed to parse beta fixture event '{event_name}': {e}; data: {data}")
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
                "ping" => BetaMessageStreamEvent::Ping,
                _ => serde_json::from_str::<BetaMessageStreamEvent>(&data).unwrap_or_else(|e| {
                    panic!("failed to parse trailing beta fixture event '{event_name}': {e}; data: {data}")
                }),
            };
            events.push(event);
        }
    }

    events
}

fn fixture_stream(fixture: &str) -> BetaMessageStream {
    let items: Vec<Result<BetaMessageStreamEvent, ApiError>> =
        parse_sse_fixture(fixture).into_iter().map(Ok).collect();
    let sse: BetaSseStream = Box::pin(stream::iter(items));
    BetaMessageStream::new(sse)
}

async fn collect_events_and_final_message(
    mut stream: BetaMessageStream,
) -> (Vec<BetaMessageStreamEvent>, BetaMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("fixture event should be valid"));
    }
    let final_message = stream
        .received_messages()
        .last()
        .expect("fixture should produce a final beta message")
        .clone();
    (events, final_message)
}

fn non_ping_event_types(events: &[BetaMessageStreamEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter(|event| !matches!(event, BetaMessageStreamEvent::Ping))
        .map(event_type_name)
        .collect()
}

#[tokio::test]
async fn beta_message_stream_accumulates_beta_specific_deltas() {
    let events = vec![
        Ok(message_start()),
        Ok(BetaMessageStreamEvent::ContentBlockStart {
            index: 0,
            content_block: BetaContentBlock::ToolUse {
                id: "toolu_1".to_owned(),
                input: serde_json::json!({}),
                name: "get_weather".to_owned(),
                caller: None,
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::InputJsonDelta {
                partial_json: r#"{"location":"San "#.to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::InputJsonDelta {
                partial_json: r#"Francisco"}"#.to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockStart {
            index: 1,
            content_block: BetaContentBlock::McpToolUse {
                id: "mcpu_1".to_owned(),
                input: serde_json::json!({}),
                name: "lookup".to_owned(),
                server_name: "weather".to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 1,
            delta: BetaContentBlockDelta::InputJsonDelta {
                partial_json: r#"{"city":"Paris"}"#.to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockStart {
            index: 2,
            content_block: BetaContentBlock::Compaction { content: None },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 2,
            delta: BetaContentBlockDelta::CompactionDelta {
                content: Some("summary ".to_owned()),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 2,
            delta: BetaContentBlockDelta::CompactionDelta {
                content: Some("text".to_owned()),
            },
        }),
        Ok(BetaMessageStreamEvent::MessageDelta {
            delta: BetaMessageDelta {
                container: None,
                stop_reason: None,
                stop_sequence: None,
            },
            usage: BetaMessageDeltaUsage {
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                input_tokens: Some(11),
                iterations: Some(vec![BetaIterationUsage::Message {
                    cache_creation: None,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                    input_tokens: 11,
                    output_tokens: 3,
                }]),
                output_tokens: 3,
                server_tool_use: None,
            },
            context_management: Some(BetaContextManagementResponse {
                applied_edits: vec![BetaContextManagementAppliedEdit::ClearThinking20251015 {
                    cleared_input_tokens: 7,
                    cleared_thinking_turns: 1,
                }],
            }),
        }),
        Ok(BetaMessageStreamEvent::MessageStop),
    ];

    let sse: BetaSseStream = Box::pin(stream::iter(events));
    let final_message = BetaMessageStream::new(sse).final_message().await.unwrap();

    assert_eq!(final_message.usage.input_tokens, 11);
    assert_eq!(final_message.usage.output_tokens, 3);
    assert_eq!(final_message.usage.iterations.as_ref().unwrap().len(), 1);
    assert_eq!(
        final_message
            .context_management
            .as_ref()
            .unwrap()
            .applied_edits
            .len(),
        1
    );

    match &final_message.content[0] {
        BetaContentBlock::ToolUse { input, .. } => {
            assert_eq!(input["location"], "San Francisco");
        }
        other => panic!("expected tool_use block, got {other:?}"),
    }

    match &final_message.content[1] {
        BetaContentBlock::McpToolUse { input, .. } => {
            assert_eq!(input["city"], "Paris");
        }
        other => panic!("expected mcp_tool_use block, got {other:?}"),
    }

    match &final_message.content[2] {
        BetaContentBlock::Compaction { content } => {
            assert_eq!(content.as_deref(), Some("summary text"));
        }
        other => panic!("expected compaction block, got {other:?}"),
    }
}

#[tokio::test]
async fn beta_message_stream_final_parsed_message_parses_streamed_text_blocks() {
    #[derive(Debug, serde::Deserialize, PartialEq)]
    struct Output {
        answer: i64,
    }

    let events = vec![
        Ok(message_start()),
        Ok(BetaMessageStreamEvent::ContentBlockStart {
            index: 0,
            content_block: BetaContentBlock::Text {
                citations: None,
                text: String::new(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::TextDelta {
                text: "{\"answer\"".to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::TextDelta {
                text: ":42}".to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockStop { index: 0 }),
        Ok(BetaMessageStreamEvent::MessageStop),
    ];

    let sse: BetaSseStream = Box::pin(stream::iter(events));
    let parsed = BetaMessageStream::new(sse)
        .final_parsed_message::<Output>()
        .await
        .unwrap();
    assert_eq!(parsed.parsed_output.unwrap(), Output { answer: 42 });
    match &parsed.content[0] {
        ParsedBetaContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().answer, 42)
        }
        other => panic!("expected parsed beta text block, got {other:?}"),
    }
}

#[tokio::test]
async fn beta_message_stream_reports_malformed_input_json_delta() {
    let events = vec![
        Ok(message_start()),
        Ok(BetaMessageStreamEvent::ContentBlockStart {
            index: 0,
            content_block: BetaContentBlock::ToolUse {
                id: "toolu_bad".to_owned(),
                input: serde_json::json!({}),
                name: "test_tool".to_owned(),
                caller: None,
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::InputJsonDelta {
                partial_json: r#"{"foo": "bar", "baz": "#.to_owned(),
            },
        }),
        Ok(BetaMessageStreamEvent::ContentBlockDelta {
            index: 0,
            delta: BetaContentBlockDelta::InputJsonDelta {
                partial_json: r#""qux": "quux"}"#.to_owned(),
            },
        }),
    ];

    let sse: BetaSseStream = Box::pin(stream::iter(events));
    let err = BetaMessageStream::new(sse)
        .final_message()
        .await
        .unwrap_err();
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
async fn beta_message_stream_handles_basic_response_fixture() {
    let fixture = include_str!("../fixtures/basic_response.txt");
    let (events, final_message) = collect_events_and_final_message(fixture_stream(fixture)).await;
    let final_text = fixture_stream(fixture).final_text().await.unwrap();

    assert_eq!(
        non_ping_event_types(&events),
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
    assert_eq!(final_message.id, "msg_4QpJur2dWWDjF6C758FbBw5vm12BaVipnK");
    assert_eq!(final_message.model, "claude-opus-4-20250514");
    assert_eq!(final_message.role, "assistant");
    assert_eq!(final_message.stop_reason, Some(BetaStopReason::EndTurn));
    assert!(final_message.stop_sequence.is_none());
    assert_eq!(final_message.type_name, "message");
    assert_eq!(final_message.usage.input_tokens, 11);
    assert_eq!(final_message.usage.output_tokens, 6);
    match &final_message.content[0] {
        BetaContentBlock::Text { text, .. } => assert_eq!(text, "Hello there!"),
        other => panic!("expected text block, got {other:?}"),
    }
    assert_eq!(final_text, "Hello there!");
}

#[tokio::test]
async fn beta_message_stream_ts_style_terminal_aliases_and_readable_stream_work() {
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
        BetaMessageStream::fromReadableStream(fixture_stream(fixture).toReadableStream())
            .finalText()
            .await
            .unwrap();
    assert_eq!(roundtrip_text, "Hello there!");
}

#[tokio::test]
async fn beta_message_stream_handles_tool_use_response_fixture() {
    let fixture = include_str!("../fixtures/tool_use_response.txt");
    let (events, final_message) = collect_events_and_final_message(fixture_stream(fixture)).await;
    let final_text = fixture_stream(fixture).final_text().await.unwrap();

    assert_eq!(
        non_ping_event_types(&events),
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
    assert_eq!(final_message.id, "msg_019Q1hrJbZG26Fb9BQhrkHEr");
    assert_eq!(final_message.model, "claude-sonnet-4-20250514");
    assert_eq!(final_message.stop_reason, Some(BetaStopReason::ToolUse));
    assert_eq!(final_message.usage.input_tokens, 377);
    assert_eq!(final_message.usage.output_tokens, 65);
    assert_eq!(final_message.usage.cache_creation_input_tokens, Some(0));
    assert_eq!(final_message.usage.cache_read_input_tokens, Some(0));
    assert_eq!(
        final_message.usage.service_tier.as_deref(),
        Some("standard")
    );
    match &final_message.content[0] {
        BetaContentBlock::Text { text, .. } => {
            assert_eq!(text, "I'll check the current weather in Paris for you.")
        }
        other => panic!("expected text block at index 0, got {other:?}"),
    }
    match &final_message.content[1] {
        BetaContentBlock::ToolUse {
            id, input, name, ..
        } => {
            assert_eq!(id, "toolu_01NRLabsLyVHZPKxbKvkfSMn");
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "Paris");
        }
        other => panic!("expected tool_use block at index 1, got {other:?}"),
    }
    assert_eq!(
        final_text,
        "I'll check the current weather in Paris for you."
    );
}

#[tokio::test]
async fn beta_message_stream_handles_incomplete_partial_json_fixture() {
    let fixture = include_str!("../fixtures/incomplete_partial_json_response.txt");
    let (events, final_message) = collect_events_and_final_message(fixture_stream(fixture)).await;

    assert_eq!(
        non_ping_event_types(&events),
        vec![
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_stop",
            "content_block_start",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "content_block_delta",
            "message_delta",
            "message_stop",
        ]
    );
    assert_eq!(final_message.id, "msg_01UdjYBBipA9omjYhicnevgq");
    assert_eq!(final_message.model, "claude-sonnet-4-5");
    assert_eq!(final_message.stop_reason, Some(BetaStopReason::MaxTokens));
    assert_eq!(final_message.usage.input_tokens, 450);
    assert_eq!(final_message.usage.output_tokens, 124);
    assert_eq!(final_message.usage.cache_creation_input_tokens, Some(0));
    assert_eq!(final_message.usage.cache_read_input_tokens, Some(0));
    assert_eq!(
        final_message.usage.service_tier.as_deref(),
        Some("standard")
    );
    assert_eq!(final_message.content.len(), 2);
    match &final_message.content[0] {
        BetaContentBlock::Text { text, .. } => assert_eq!(
            text,
            "I'll create a comprehensive tax guide for someone with multiple W2s and save it in a file called taxes.txt. Let me do that for you now."
        ),
        other => panic!("expected text block at index 0, got {other:?}"),
    }
    match &final_message.content[1] {
        BetaContentBlock::ToolUse {
            id, input, name, ..
        } => {
            assert_eq!(id, "toolu_01EKqbqmZrGRXy18eN7m9kvY");
            assert_eq!(name, "make_file");
            assert_eq!(input["filename"], "taxes.txt");
            assert_eq!(
                input["lines_of_text"][0],
                "# COMPREHENSIVE TAX GUIDE FOR INDIVIDUALS WITH MULTIPLE W-2s"
            );
        }
        other => panic!("expected tool_use block at index 1, got {other:?}"),
    }
}

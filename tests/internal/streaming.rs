// Migrated coverage from TS SDK tests/streaming.test.ts for the public
// Stream.fromSSEResponse equivalent exposed as core::streaming::from_sse_response.

use anthropic_sdk::core::error::ApiError;
use anthropic_sdk::core::streaming::{
    _iterSSEMessages, Stream as AnthropicStream, from_readable_stream, from_sse_response,
    fromReadableStream, fromSSEResponse, iter_sse_messages, to_readable_stream, toReadableStream,
};
use bytes::Bytes;
use futures::{StreamExt, stream};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn sse_response(body: &str) -> reqwest::Response {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body),
        )
        .expect(1)
        .mount(&server)
        .await;

    reqwest::Client::new()
        .get(format!("{}/sse", server.uri()))
        .send()
        .await
        .expect("SSE response should be returned")
}

#[tokio::test]
async fn raw_sse_iterator_matches_ts_iter_sse_messages_for_data_and_event_edges() {
    let response = sse_response(
        "data: {\"foo\":true}\n\
         \n\
         event: foo\n\
         \n\
         event: ping\n\
         data: {\n\
         data: \"bar\":\n\
         data: \n\
         data:\n\
         data: false}\n\
         \n",
    )
    .await;

    let mut stream = iter_sse_messages(response);

    let first = stream.next().await.unwrap().unwrap();
    assert!(first.event.is_none());
    assert_eq!(first.data, "{\"foo\":true}");
    assert_eq!(first.raw, vec!["data: {\"foo\":true}".to_owned()]);

    let second = stream.next().await.unwrap().unwrap();
    assert_eq!(second.event.as_deref(), Some("foo"));
    assert_eq!(second.data, "");

    let third = stream.next().await.unwrap().unwrap();
    assert_eq!(third.event.as_deref(), Some("ping"));
    assert_eq!(third.data, "{\n\"bar\":\n\n\nfalse}");

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn raw_sse_iterator_ts_style_alias_matches_iter_sse_messages() {
    let response = sse_response("event: completion\ndata: {\"foo\":true}\n\n").await;
    let mut stream = _iterSSEMessages(response);
    let event = stream.next().await.unwrap().unwrap();

    assert_eq!(event.event.as_deref(), Some("completion"));
    assert_eq!(event.data, "{\"foo\":true}");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn raw_sse_iterator_handles_escaped_double_newline_and_special_chars_like_ts() {
    let escaped = sse_response(
        "event: ping\n\
         data: {\"foo\": \"my long\\n\\ncontent\"}\n\
         \n",
    )
    .await;
    let mut escaped_stream = iter_sse_messages(escaped);
    let event = escaped_stream.next().await.unwrap().unwrap();
    assert_eq!(event.event.as_deref(), Some("ping"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&event.data).unwrap(),
        json!({"foo": "my long\n\ncontent"})
    );
    assert!(escaped_stream.next().await.is_none());

    let line_separator = "\u{2028}";
    let special = sse_response(&format!(
        "data: {{\"content\": \"culpa \"}}\n\ndata: {{\"content\": \"{line_separator}\"}}\n\ndata: {{\"content\": \"foo\"}}\n\n"
    ))
    .await;
    let mut special_stream = iter_sse_messages(special);
    let first = special_stream.next().await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&first.data).unwrap(),
        json!({"content": "culpa "})
    );
    let second = special_stream.next().await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&second.data).unwrap(),
        json!({"content": line_separator})
    );
    let third = special_stream.next().await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&third.data).unwrap(),
        json!({"content": "foo"})
    );
    assert!(special_stream.next().await.is_none());
}

#[tokio::test]
async fn sse_stream_decodes_completion_events_and_skips_ping() {
    let response = sse_response(
        "event: completion\n\
         data: {\"foo\":true}\n\
         \n\
         event: ping\n\
         data: {}\n\
         \n\
         event: message_delta\n\
         data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\
         \n",
    )
    .await;

    let mut stream = from_sse_response::<serde_json::Value>(response);

    let first = stream
        .next()
        .await
        .expect("first event")
        .expect("first event should parse");
    assert_eq!(first, json!({"foo": true}));

    let second = stream
        .next()
        .await
        .expect("second event")
        .expect("second event should parse");
    assert_eq!(
        second,
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}})
    );

    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn sse_stream_camel_case_alias_matches_ts_from_sse_response_name() {
    let response = sse_response("event: completion\ndata: {\"foo\":true}\n\n").await;
    let mut stream = fromSSEResponse::<serde_json::Value>(response);

    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"foo": true}));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn sse_error_event_message_matches_ts_api_error_snapshot() {
    let error_body =
        r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
    let response = sse_response(&format!("event: error\ndata: {error_body}\n\n")).await;

    let mut stream = from_sse_response::<serde_json::Value>(response);
    let err = stream
        .next()
        .await
        .expect("error event should yield an error")
        .expect_err("error event should not parse as an item");

    assert!(matches!(err, ApiError::Connection { .. }));
    assert_eq!(err.to_string(), error_body);
}

#[tokio::test]
async fn readable_stream_decodes_newline_json_like_ts_from_readable_stream() {
    let chunks = stream::iter(vec![
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"{\"foo\":true}\n")),
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"\n{\"bar\":")),
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"false}\n")),
    ]);

    let mut stream = from_readable_stream::<serde_json::Value, _, _>(chunks);

    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"foo": true}));
    assert_eq!(stream.next().await.unwrap().unwrap(), json!({"bar": false}));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn readable_stream_round_trips_json_values_like_ts_to_readable_stream() {
    let values = stream::iter(vec![
        Ok::<_, ApiError>(json!({"foo": true})),
        Ok::<_, ApiError>(json!({"bar": false})),
    ]);
    let bytes = to_readable_stream(values);
    let mut decoded = from_readable_stream::<serde_json::Value, _, _>(bytes);

    assert_eq!(decoded.next().await.unwrap().unwrap(), json!({"foo": true}));
    assert_eq!(
        decoded.next().await.unwrap().unwrap(),
        json!({"bar": false})
    );
    assert!(decoded.next().await.is_none());
}

#[tokio::test]
async fn readable_stream_camel_case_aliases_match_ts_names() {
    let values = stream::iter(vec![Ok::<_, ApiError>(json!({"foo": true}))]);
    let bytes = toReadableStream(values);
    let mut decoded = fromReadableStream::<serde_json::Value, _, _>(bytes);

    assert_eq!(decoded.next().await.unwrap().unwrap(), json!({"foo": true}));
    assert!(decoded.next().await.is_none());
}

#[tokio::test]
async fn stream_namespace_static_helpers_match_ts_stream_class_names() {
    let values = stream::iter(vec![Ok::<_, ApiError>(json!({"foo": true}))]);
    let bytes = AnthropicStream::toReadableStream(values);
    let mut decoded = AnthropicStream::fromReadableStream::<serde_json::Value, _, _>(bytes);
    assert_eq!(decoded.next().await.unwrap().unwrap(), json!({"foo": true}));
    assert!(decoded.next().await.is_none());

    let response = sse_response("event: completion\ndata: {\"ok\":true}\n\n").await;
    let mut sse = AnthropicStream::fromSSEResponse::<serde_json::Value>(response);
    assert_eq!(sse.next().await.unwrap().unwrap(), json!({"ok": true}));
    assert!(sse.next().await.is_none());
}

#[tokio::test]
async fn readable_stream_preserves_multibyte_characters_across_chunks() {
    let chunks = stream::iter(vec![
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"{\"content\":\"")),
        Ok::<Bytes, std::io::Error>(Bytes::from(vec![0xd0])),
        Ok::<Bytes, std::io::Error>(Bytes::from(vec![
            0xb8, 0xd0, 0xb7, 0xd0, 0xb2, 0xd0, 0xb5, 0xd1, 0x81, 0xd1, 0x82, 0xd0, 0xbd, 0xd0,
            0xb8,
        ])),
        Ok::<Bytes, std::io::Error>(Bytes::from_static(b"\"}\n")),
    ]);

    let mut stream = from_readable_stream::<serde_json::Value, _, _>(chunks);
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        json!({"content": "известни"})
    );
    assert!(stream.next().await.is_none());
}

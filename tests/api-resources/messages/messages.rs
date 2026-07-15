// Integration tests for the Messages API resource.
//
// Ported from TS SDK: tests/api-resources/messages/messages.test.ts
//
// Uses wiremock to provide a local mock server, verifying that the client
// correctly serializes request parameters and deserializes responses.

use std::time::Duration;

use anthropic_sdk::resources::messages::types as messages_types;
use anthropic_sdk::{
    Anthropic, ClientOptions, MessageContent, MessageCountTokensParams, MessageCreateParams,
    MessageParam, RequestOptions,
};
use futures::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a client pointing at the given mock server URL.
fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

/// A minimal valid Message JSON response.
#[tokio::test]
async fn messages_create_captures_request_id_like_ts_object_response() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_xxx")
                .set_body_json(message_response_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![],
        model: "claude-opus-4-20250514".to_owned(),
        ..Default::default()
    };

    let msg = client.messages().create(&params).await.unwrap();
    assert_eq!(msg.id, "msg_test123");
    assert_eq!(msg.request_id.as_deref(), Some("req_xxx"));
    let serialized = serde_json::to_value(&msg).unwrap();
    assert!(serialized.get("_request_id").is_none());
}

#[tokio::test]
async fn messages_create_with_response_returns_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_with_response")
                .set_body_json(message_response_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![],
        model: "claude-opus-4-20250514".to_owned(),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_response(&params)
        .await
        .unwrap();
    assert_eq!(response.request_id.as_deref(), Some("req_with_response"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_with_response")
    );
    assert_eq!(response.data.id, "msg_test123");
    assert_eq!(
        response.data.request_id.as_deref(),
        Some("req_with_response")
    );
    assert_eq!(
        response.response.json::<serde_json::Value>().unwrap()["id"],
        "msg_test123"
    );
}

#[test]
fn messages_ts_exported_aliases_are_available() {
    let _: messages_types::Model = "claude-opus-4-6".to_owned();
    let _: messages_types::JSONOutputFormat = messages_types::JsonOutputFormat::default();
    let _: messages_types::MessageCreateParamsBase = messages_types::MessageCreateParams::default();
    let _: messages_types::MessageCreateParamsNonStreaming =
        messages_types::MessageCreateParams::default();
    let _: messages_types::MessageCreateParamsStreaming =
        messages_types::MessageCreateParams::default();
    let _: messages_types::MessageStreamParams = messages_types::MessageCreateParams::default();
    let _: messages_types::Base64PDFSource = messages_types::DocumentSource::Base64Pdf {
        data: "AAAA".to_owned(),
        media_type: "application/pdf".to_owned(),
    };
    let _: messages_types::ToolChoiceAny = messages_types::ToolChoice::Any {
        disable_parallel_tool_use: Some(true),
    };
    let _: messages_types::TextDelta = messages_types::ContentBlockDelta::TextDelta {
        text: "hello".to_owned(),
    };
    let citation = messages_types::TextCitation::CharLocation {
        cited_text: "quoted".to_owned(),
        document_index: 0,
        document_title: Some("doc".to_owned()),
        end_char_index: 6,
        file_id: Some("file_123".to_owned()),
        start_char_index: 0,
    };
    let _: messages_types::CitationsDelta = messages_types::CitationsDeltaData {
        type_name: "citations_delta".to_owned(),
        citation: citation.clone(),
    };
    let delta: messages_types::ContentBlockDelta = serde_json::from_value(serde_json::json!({
        "type": "citations_delta",
        "citation": {
            "type": "char_location",
            "cited_text": "quoted",
            "document_index": 0,
            "document_title": "doc",
            "end_char_index": 6,
            "file_id": "file_123",
            "start_char_index": 0
        }
    }))
    .unwrap();
    match delta {
        messages_types::ContentBlockDelta::CitationsDelta { citation: parsed } => {
            assert!(matches!(
                parsed,
                messages_types::TextCitation::CharLocation { .. }
            ));
        }
        other => panic!("expected citations delta, got {other:?}"),
    }
    let _: messages_types::ToolInputSchema = serde_json::json!({"type": "object"});
    let _: messages_types::InputSchema = serde_json::json!({"type": "object"});
    let _: messages_types::RawMessageDeltaEventDelta = messages_types::MessageDelta {
        stop_reason: None,
        stop_sequence: None,
    };
    let _: messages_types::Delta = messages_types::MessageDelta {
        stop_reason: None,
        stop_sequence: None,
    };
    let _: messages_types::WebSearchTool20250305UserLocation =
        messages_types::WebSearchUserLocation {
            type_name: "approximate".to_owned(),
            city: None,
            country: None,
            region: None,
            timezone: None,
        };
    let _: messages_types::UserLocation = messages_types::WebSearchUserLocation {
        type_name: "approximate".to_owned(),
        city: None,
        country: None,
        region: None,
        timezone: None,
    };

    let strict_tool = messages_types::Tool {
        input_schema: serde_json::json!({"type": "object"}),
        name: "strict_tool".to_owned(),
        cache_control: None,
        description: None,
        eager_input_streaming: None,
        strict: Some(true),
        type_name: Some("custom".to_owned()),
    };
    let json = serde_json::to_value(strict_tool).unwrap();
    assert_eq!(json["strict"], true);
}

fn message_response_json() -> serde_json::Value {
    serde_json::json!({
        "id": "msg_test123",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": "Hello!"}],
        "model": "claude-opus-4-6",
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 10,
            "output_tokens": 5
        }
    })
}

/// A minimal valid MessageTokensCount JSON response.
fn count_tokens_response_json() -> serde_json::Value {
    serde_json::json!({
        "input_tokens": 42
    })
}

/// A valid SSE stream response for streaming tests.
fn streaming_response_body() -> String {
    let events = [
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi!\"}}\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3}}\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n",
    ];
    events.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Maps to: TS "create: only required params"
///
/// Verifies that creating a message with only the required parameters
/// (max_tokens, messages, model) succeeds and the response is deserialized
/// correctly.
#[tokio::test]
async fn test_create_with_required_params_only() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_response_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![MessageParam {
            content: MessageContent::Text("Hello, world".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };

    let response = client.messages().create(&params).await;
    assert!(
        response.is_ok(),
        "create should succeed: {:?}",
        response.err()
    );

    let msg = response.unwrap();
    assert_eq!(msg.id, "msg_test123");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.model, "claude-opus-4-6");
    assert_eq!(msg.content.len(), 1);
}

#[tokio::test]
async fn messages_create_rejects_nonstreaming_requests_that_need_streaming_timeout() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 25_000,
        messages: vec![MessageParam {
            content: MessageContent::Text("Write a very long answer".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };

    let err = client.messages().create(&params).await.unwrap_err();
    assert!(err
        .to_string()
        .contains("https://github.com/anthropics/anthropic-sdk-typescript#long-requests"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn messages_create_rejects_known_model_nonstreaming_token_cap() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 9_000,
        messages: vec![MessageParam {
            content: MessageContent::Text("Write a long answer".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-20250514".to_owned(),
        ..Default::default()
    };

    let err = client.messages().create(&params).await.unwrap_err();
    assert_eq!(
        err.to_string(),
        "SDK error: Streaming is required for operations that may take longer than 10 minutes. See https://github.com/anthropics/anthropic-sdk-typescript#long-requests for more details"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn messages_create_allows_long_nonstreaming_request_with_client_timeout_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_response_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        timeout: Some(30 * 60 * 1000),
        ..Default::default()
    })
    .expect("client creation should succeed");
    let params = MessageCreateParams {
        max_tokens: 25_000,
        messages: vec![MessageParam {
            content: MessageContent::Text("Write a very long answer".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };

    let msg = client.messages().create(&params).await.unwrap();
    assert_eq!(msg.id, "msg_test123");
}

#[tokio::test]
async fn messages_create_request_timeout_does_not_bypass_streaming_requirement_like_ts_ordering() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 25_000,
        messages: vec![MessageParam {
            content: MessageContent::Text("Write a very long answer".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };
    let options = RequestOptions {
        timeout: Some(Duration::from_secs(30 * 60)),
        ..Default::default()
    };

    let err = client
        .messages()
        .create_with_options(&params, Some(&options))
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("https://github.com/anthropics/anthropic-sdk-typescript#long-requests"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Maps to: TS "create: required and optional params"
///
/// Verifies that the client correctly serializes all optional parameters
/// and the request succeeds.
#[tokio::test]
async fn test_create_with_all_optional_params() {
    use anthropic_sdk::{
        Effort, JsonOutputFormat, Metadata, OutputConfig, SystemPrompt, TextBlockParam,
        ThinkingConfig, Tool, ToolChoice, ToolUnion,
    };

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_response_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![MessageParam {
            content: MessageContent::Text("Hello, world".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        metadata: Some(Metadata {
            user_id: Some("13803d75-b4b5-4c3e-b2a2-6f21399b021b".to_owned()),
        }),
        output_config: Some(OutputConfig {
            effort: Some(Effort::Low),
            format: Some(JsonOutputFormat {
                schema: serde_json::json!({"foo": "bar"}),
                type_name: "json_schema".to_owned(),
            }),
        }),
        service_tier: Some("auto".to_owned()),
        stop_sequences: Some(vec!["stop".to_owned()]),
        stream: Some(false),
        system: Some(SystemPrompt::Blocks(vec![TextBlockParam {
            text: "Today's date is 2024-06-01.".to_owned(),
            cache_control: None,
            citations: None,
            type_name: Some("text".to_owned()),
        }])),
        temperature: Some(1.0),
        thinking: Some(ThinkingConfig::Enabled {
            budget_tokens: 1024,
        }),
        tool_choice: Some(ToolChoice::Auto {
            disable_parallel_tool_use: Some(true),
        }),
        tools: Some(vec![ToolUnion::Custom(Tool {
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"},
                    "unit": {"type": "string"}
                },
                "required": ["location"]
            }),
            name: "get_weather".to_owned(),
            cache_control: None,
            description: Some("Get the current weather in a given location".to_owned()),
            eager_input_streaming: None,
            strict: None,
            type_name: Some("custom".to_owned()),
        })]),
        top_k: Some(5),
        top_p: Some(0.7),
        ..Default::default()
    };

    let response = client.messages().create(&params).await;
    assert!(
        response.is_ok(),
        "create with all params should succeed: {:?}",
        response.err()
    );

    let msg = response.unwrap();
    assert_eq!(msg.id, "msg_test123");
}

/// Maps to: TS "countTokens: only required params"
///
/// Verifies that the count_tokens endpoint is called correctly and the
/// response is deserialized.
#[tokio::test]
async fn messages_create_sends_helper_header_for_marked_tool() {
    use anthropic_sdk::{Tool, ToolUnion};

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_response_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![MessageParam {
            content: MessageContent::Text("Hello, world".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        tools: Some(vec![ToolUnion::Custom(Tool {
            input_schema: serde_json::json!({"type": "object"}),
            name: "helper_tool".to_owned(),
            cache_control: None,
            description: Some("Helper-created tool".to_owned()),
            eager_input_streaming: None,
            strict: None,
            type_name: Some("custom".to_owned()),
        })
        .with_stainless_helper("schemarsTool")]),
        ..Default::default()
    };

    let response = client.messages().create(&params).await.unwrap();
    assert_eq!(response.id, "msg_test123");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|value| value.to_str().ok()),
        Some("schemarsTool")
    );

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["tools"][0]["name"], "helper_tool");
    assert!(body["tools"][0].get("__stainless_helper").is_none());
}

/// Maps to: TS "countTokens: only required params"
///
/// Verifies that the count_tokens endpoint is called correctly and the
/// response is deserialized.
#[tokio::test]
async fn test_count_tokens() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(count_tokens_response_json()))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCountTokensParams {
        messages: vec![MessageParam {
            content: MessageContent::Text("Hello".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        output_config: None,
        system: None,
        thinking: None,
        tool_choice: None,
        tools: None,
    };

    let response = client.messages().countTokens(&params).await;
    assert!(
        response.is_ok(),
        "count_tokens should succeed: {:?}",
        response.err()
    );

    let result = response.unwrap();
    assert_eq!(result.input_tokens, 42);
}

#[tokio::test]
async fn messages_count_tokens_with_response_returns_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_count")
                .set_body_json(count_tokens_response_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCountTokensParams {
        messages: vec![MessageParam {
            content: MessageContent::Text("Hello".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        output_config: None,
        system: None,
        thinking: None,
        tool_choice: None,
        tools: None,
    };

    let response = client
        .messages()
        .countTokensWithResponse(&params)
        .await
        .unwrap();
    assert_eq!(response.data.input_tokens, 42);
    assert_eq!(response.request_id.as_deref(), Some("req_count"));
    assert_eq!(response.response.header("request-id"), Some("req_count"));
}

/// Maps to: TS "stream returns MessageStream"
///
/// Verifies that the stream() method returns a MessageStream that can be
/// consumed to get a final message.
#[tokio::test]
async fn test_stream_returns_message_stream() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(streaming_response_body()),
        )
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![MessageParam {
            content: MessageContent::Text("Say hi".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };

    let stream = client.messages().stream(&params).await;
    assert!(
        stream.is_ok(),
        "stream() should succeed: {:?}",
        stream.err()
    );

    let ms = stream.unwrap();
    let final_text = ms.final_text().await;
    assert!(
        final_text.is_ok(),
        "final_text should succeed: {:?}",
        final_text.err()
    );
    assert_eq!(final_text.unwrap(), "Hi!");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper-method")
            .and_then(|v| v.to_str().ok()),
        Some("stream")
    );
}

#[tokio::test]
async fn messages_stream_with_response_helpers_return_stream_response_and_request_id_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("request-id", "req_stream_raw")
                .set_body_string(streaming_response_body()),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("request-id", "req_stream_high")
                .set_body_string(streaming_response_body()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = MessageCreateParams {
        max_tokens: 1024,
        messages: vec![MessageParam {
            content: MessageContent::Text("Say hi".to_owned()),
            role: "user".to_owned(),
        }],
        model: "claude-opus-4-6".to_owned(),
        ..Default::default()
    };

    let raw = client
        .messages()
        .create_stream_with_response(&params)
        .await
        .unwrap();
    assert_eq!(raw.request_id.as_deref(), Some("req_stream_raw"));
    assert_eq!(raw.response.header("request-id"), Some("req_stream_raw"));
    assert!(raw.response.body.is_empty());
    let first = raw.data.take(1).next().await.unwrap().unwrap();
    assert!(matches!(
        first,
        messages_types::MessageStreamEvent::MessageStart { .. }
    ));

    let high = client
        .messages()
        .stream_with_response(&params)
        .await
        .unwrap();
    assert_eq!(high.request_id.as_deref(), Some("req_stream_high"));
    assert_eq!(high.response.header("request-id"), Some("req_stream_high"));
    assert!(high.response.body.is_empty());
    assert_eq!(high.data.final_text().await.unwrap(), "Hi!");
}

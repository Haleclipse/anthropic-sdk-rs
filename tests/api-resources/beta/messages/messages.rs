// Integration/parity tests for beta Messages resource methods.
//
// Mirrors TS SDK tests/api-resources/beta/messages behavior that is not purely
// type-level.

use anthropic_sdk::helpers::beta::json_schema::{
    beta_json_schema_format, beta_json_schema_output_format,
};
use anthropic_sdk::helpers::beta::mcp::{
    MCPPromptMessageLike, MCPToolLike, MCPToolResultContentLike, mcp_message, mcp_tool_definition,
};
use anthropic_sdk::resources::beta::messages::{
    BetaMessageContent, BetaMessageCountTokensParams, BetaMessageCreateParams, BetaMessageParam,
    BetaMessageStreamEvent, BetaToolInputSchema, BetaToolUnion,
};
use anthropic_sdk::{Anthropic, ClientOptions};
use futures::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: "test-api-key".into(),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

fn user_message() -> BetaMessageParam {
    BetaMessageParam {
        role: "user".to_owned(),
        content: BetaMessageContent::Text("Hello".to_owned()),
    }
}

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answer": {"type": "string"}
        },
        "required": ["answer"]
    })
}

fn beta_message_json() -> serde_json::Value {
    serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-3-5-sonnet-latest",
        "content": [],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 5, "output_tokens": 7}
    })
}

fn beta_streaming_response_body() -> String {
    let events = [
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-3-5-sonnet-latest\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" there\"}}\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":3},\"context_management\":null}\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n",
    ];
    events.join("\n")
}

#[tokio::test]
async fn beta_messages_create_transforms_deprecated_output_format_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        output_format: Some(beta_json_schema_format(schema()).unwrap()),
        ..Default::default()
    };

    let message = client.beta().messages().create(&params).await.unwrap();
    assert_eq!(message.id, "msg_123");

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], false);
    assert!(body.get("output_format").is_none());
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert!(body["output_config"]["format"]["schema"].is_object());
}

#[tokio::test]
async fn beta_messages_create_captures_request_id_like_ts_object_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta")
                .set_body_json(beta_message_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        ..Default::default()
    };

    let message = client.beta().messages().create(&params).await.unwrap();
    assert_eq!(message.request_id.as_deref(), Some("req_beta"));
    let serialized = serde_json::to_value(&message).unwrap();
    assert!(serialized.get("_request_id").is_none());
}

#[tokio::test]
async fn beta_messages_create_with_response_returns_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_with")
                .set_body_json(beta_message_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        ..Default::default()
    };

    let response = client
        .beta()
        .messages()
        .create_with_response(&params)
        .await
        .unwrap();
    assert_eq!(response.request_id.as_deref(), Some("req_beta_with"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_beta_with")
    );
    assert_eq!(response.data.id, "msg_123");
    assert_eq!(response.data.request_id.as_deref(), Some("req_beta_with"));
}

#[tokio::test]
async fn beta_messages_create_sends_helper_header_for_mcp_tool() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let tool = mcp_tool_definition(
        MCPToolLike {
            name: "lookup".to_owned(),
            description: Some("Lookup via MCP".to_owned()),
            input_schema: BetaToolInputSchema::default(),
        },
        None,
    );
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        tools: Some(vec![BetaToolUnion::Custom(tool)]),
        betas: Some(vec!["custom-beta".to_owned()]),
        ..Default::default()
    };

    client.beta().messages().create(&params).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("mcpTool")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("anthropic-beta")
            .and_then(|v| v.to_str().ok()),
        Some("custom-beta")
    );

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["tools"][0]["name"], "lookup");
    assert!(body["tools"][0].get("stainless_helper").is_none());
}

#[tokio::test]
async fn beta_messages_create_sends_helper_header_for_mcp_message_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let message = mcp_message(MCPPromptMessageLike {
        role: "user".to_owned(),
        content: MCPToolResultContentLike::Text {
            text: "hello from MCP".to_owned(),
        },
    })
    .unwrap();
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![message],
        ..Default::default()
    };

    client.beta().messages().create(&params).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let helper = requests[0]
        .headers
        .get("x-stainless-helper")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(helper.contains("mcpMessage"), "helper header: {helper}");
    assert!(helper.contains("mcpContent"), "helper header: {helper}");

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["messages"][0]["content"][0]["text"], "hello from MCP");
    assert!(
        body["messages"][0]["content"][0]
            .get("stainless_helpers")
            .is_none()
    );
}

#[tokio::test]
async fn beta_messages_create_rejects_nonstreaming_requests_that_need_streaming_timeout() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 25_000,
        messages: vec![user_message()],
        ..Default::default()
    };

    let err = client.beta().messages().create(&params).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("https://github.com/anthropics/anthropic-sdk-typescript#long-requests")
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn beta_messages_stream_returns_high_level_beta_message_stream() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(beta_streaming_response_body()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        ..Default::default()
    };

    let stream = client.beta().messages().stream(&params).await.unwrap();
    assert_eq!(stream.final_text().await.unwrap(), "Hi there");

    let requests = server.received_requests().await.unwrap();
    let query: std::collections::HashMap<_, _> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper-method")
            .and_then(|v| v.to_str().ok()),
        Some("stream")
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], true);
}

#[tokio::test]
async fn beta_messages_stream_with_response_helpers_return_stream_response_and_request_id_like_ts()
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("request-id", "req_beta_stream_raw")
                .set_body_string(beta_streaming_response_body()),
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
                .insert_header("request-id", "req_beta_stream_high")
                .set_body_string(beta_streaming_response_body()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        ..Default::default()
    };

    let raw = client
        .beta()
        .messages()
        .create_stream_with_response(&params)
        .await
        .unwrap();
    assert_eq!(raw.request_id.as_deref(), Some("req_beta_stream_raw"));
    assert_eq!(
        raw.response.header("request-id"),
        Some("req_beta_stream_raw")
    );
    assert!(raw.response.body.is_empty());
    let first = raw.data.take(1).next().await.unwrap().unwrap();
    assert!(matches!(first, BetaMessageStreamEvent::MessageStart { .. }));

    let high = client
        .beta()
        .messages()
        .stream_with_response(&params)
        .await
        .unwrap();
    assert_eq!(high.request_id.as_deref(), Some("req_beta_stream_high"));
    assert_eq!(
        high.response.header("request-id"),
        Some("req_beta_stream_high")
    );
    assert!(high.response.body.is_empty());
    assert_eq!(high.data.final_text().await.unwrap(), "Hi there");
}

#[tokio::test]
async fn beta_messages_count_tokens_transforms_deprecated_output_format_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "input_tokens": 42
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCountTokensParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![user_message()],
        betas: Some(vec!["custom-beta".to_owned()]),
        output_format: Some(beta_json_schema_format(schema()).unwrap()),
        ..Default::default()
    };

    let count = client.beta().messages().countTokens(&params).await.unwrap();
    assert_eq!(count.input_tokens, 42);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "custom-beta,token-counting-2024-11-01"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("output_format").is_none());
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
}

#[tokio::test]
async fn beta_messages_count_tokens_with_response_returns_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_count")
                .set_body_json(serde_json::json!({
                    "input_tokens": 42
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaMessageCountTokensParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        messages: vec![user_message()],
        ..Default::default()
    };

    let response = client
        .beta()
        .messages()
        .countTokensWithResponse(&params)
        .await
        .unwrap();
    assert_eq!(response.data.input_tokens, 42);
    assert_eq!(response.request_id.as_deref(), Some("req_beta_count"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_beta_count")
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "token-counting-2024-11-01"
    );
}

#[tokio::test]
async fn beta_messages_create_rejects_output_format_and_output_config_format() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());
    let params = BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 64,
        messages: vec![user_message()],
        output_config: Some(beta_json_schema_output_format(schema()).unwrap()),
        output_format: Some(beta_json_schema_format(schema()).unwrap()),
        ..Default::default()
    };

    let err = client.beta().messages().create(&params).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("Both output_format and output_config.format were provided")
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

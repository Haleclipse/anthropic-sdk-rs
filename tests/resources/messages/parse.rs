// Integration tests for Messages.parse().
//
// Ported from TS SDK: tests/resources/messages/parse.test.ts

use anthropic_sdk::helpers::json_schema::json_schema_output_format;
use anthropic_sdk::{
    Anthropic, ClientOptions, MessageContent, MessageCreateParams, MessageParam, ParsedContentBlock,
};
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Debug, Deserialize, PartialEq)]
struct Weather {
    city: String,
    temperature: i64,
    conditions: Vec<String>,
}

fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: "test-api-key".into(),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

fn parse_params() -> MessageCreateParams {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "city": {"type": "string"},
            "temperature": {"type": "integer"},
            "conditions": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
        "required": ["city", "temperature", "conditions"]
    });

    MessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 1024,
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("What is the weather in SF?".to_owned()),
        }],
        output_config: Some(json_schema_output_format(schema).unwrap()),
        ..Default::default()
    }
}

fn message_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-3-5-sonnet-latest",
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 10, "output_tokens": 25}
    })
}

#[tokio::test]
async fn messages_parse_parses_structured_output_and_sends_output_config() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_json(
            r#"{"city":"San Francisco","temperature":72,"conditions":["sunny","clear"]}"#,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let parsed = client
        .messages()
        .parse::<Weather>(&parse_params())
        .await
        .unwrap();

    assert_eq!(
        parsed.parsed_output,
        Some(Weather {
            city: "San Francisco".to_owned(),
            temperature: 72,
            conditions: vec!["sunny".to_owned(), "clear".to_owned()],
        })
    );
    assert_eq!(parsed.message.id, "msg_123");

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], false);
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert!(body["output_config"]["format"]["schema"].is_object());
}

#[tokio::test]
async fn messages_parse_with_response_returns_parsed_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_parse")
                .set_body_json(message_json(
                    r#"{"city":"San Francisco","temperature":72,"conditions":["sunny","clear"]}"#,
                )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .messages()
        .parse_with_response::<Weather>(&parse_params())
        .await
        .unwrap();

    assert_eq!(response.request_id.as_deref(), Some("req_parse"));
    assert_eq!(response.response.header("request-id"), Some("req_parse"));
    assert_eq!(
        response.response.json::<serde_json::Value>().unwrap()["id"],
        "msg_123"
    );
    assert_eq!(
        response.data.parsed_output,
        Some(Weather {
            city: "San Francisco".to_owned(),
            temperature: 72,
            conditions: vec!["sunny".to_owned(), "clear".to_owned()],
        })
    );
}

#[tokio::test]
async fn messages_parse_returns_validation_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_json(
            r#"{"city":"San Francisco","temperature":"hot","conditions":["sunny"]}"#,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let err = client
        .messages()
        .parse::<Weather>(&parse_params())
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Failed to parse structured output")
    );
}

#[tokio::test]
async fn messages_parse_without_json_schema_returns_null_parsed_output_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message_json("plain text")))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut params = parse_params();
    params.output_config = None;

    let parsed = client.messages().parse::<Weather>(&params).await.unwrap();
    assert!(parsed.parsed_output.is_none());
    match &parsed.content[0] {
        ParsedContentBlock::Text { parsed_output, .. } => assert!(parsed_output.is_none()),
        other => panic!("expected text block, got {other:?}"),
    }
}

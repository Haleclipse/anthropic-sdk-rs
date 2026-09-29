// Integration tests for beta Messages.parse().
//
// Ported from TS SDK: tests/resources/beta/messages/parse.test.ts

use anthropic_sdk::helpers::beta::json_schema::beta_json_schema_output_format;
use anthropic_sdk::resources::beta::messages::{
    BetaMessageContent, BetaMessageCreateParams, BetaMessageParam,
};
use anthropic_sdk::sdk_lib::beta_parser::ParsedBetaContentBlock;
use anthropic_sdk::{Anthropic, ClientOptions};
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

fn beta_parse_params() -> BetaMessageCreateParams {
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

    BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 1024,
        messages: vec![BetaMessageParam {
            role: "user".to_owned(),
            content: BetaMessageContent::Text("What is the weather in SF?".to_owned()),
        }],
        betas: Some(vec!["custom-beta".to_owned()]),
        output_config: Some(beta_json_schema_output_format(schema).unwrap()),
        container: None,
        context_management: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_format: None,
        service_tier: None,
        speed: None,
        stop_sequences: None,
        stream: None,
        system: None,
        temperature: None,
        thinking: None,
        tool_choice: None,
        tools: None,
        top_k: None,
        top_p: None,
    }
}

fn beta_message_json(text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "msg_123",
        "type": "message",
        "role": "assistant",
        "model": "claude-3-5-sonnet-latest",
        "container": null,
        "context_management": null,
        "content": [{"type": "text", "text": text}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 10, "output_tokens": 25}
    })
}

#[tokio::test]
async fn beta_messages_parse_parses_structured_output_and_adds_beta_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json(
            r#"{"city":"San Francisco","temperature":72,"conditions":["sunny","clear"]}"#,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let parsed = client
        .beta()
        .messages()
        .parse::<Weather>(&beta_parse_params())
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
    let query: std::collections::HashMap<_, _> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "custom-beta,structured-outputs-2025-12-15"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], false);
    assert!(body.get("betas").is_none());
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
}

#[tokio::test]
async fn beta_messages_parse_with_response_returns_parsed_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_parse")
                .set_body_json(beta_message_json(
                    r#"{"city":"San Francisco","temperature":72,"conditions":["sunny","clear"]}"#,
                )),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .beta()
        .messages()
        .parse_with_response::<Weather>(&beta_parse_params())
        .await
        .unwrap();

    assert_eq!(response.request_id.as_deref(), Some("req_beta_parse"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_beta_parse")
    );
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

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "custom-beta,structured-outputs-2025-12-15"
    );
}

#[tokio::test]
async fn beta_messages_parse_returns_validation_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json(
            r#"{"city":"San Francisco","temperature":"hot","conditions":["sunny"]}"#,
        )))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let err = client
        .beta()
        .messages()
        .parse::<Weather>(&beta_parse_params())
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("Failed to parse structured output"));
}

#[tokio::test]
async fn beta_messages_parse_without_json_schema_returns_null_parsed_output_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_message_json("plain text")))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut params = beta_parse_params();
    params.output_config = None;

    let parsed = client
        .beta()
        .messages()
        .parse::<Weather>(&params)
        .await
        .unwrap();
    assert!(parsed.parsed_output.is_none());
    match &parsed.content[0] {
        ParsedBetaContentBlock::Text { parsed_output, .. } => assert!(parsed_output.is_none()),
        other => panic!("expected beta text block, got {other:?}"),
    }

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "custom-beta,structured-outputs-2025-12-15"
    );
}

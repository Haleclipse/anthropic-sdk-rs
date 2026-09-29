// Integration tests for legacy Completions API resources.
//
// Ported from TS SDK: tests/api-resources/completions.test.ts

use std::any::TypeId;

use anthropic_sdk::resources::completions::{
    CompletionCreateParams, CompletionCreateParamsBase, CompletionCreateParamsNonStreaming,
    CompletionCreateParamsStreaming, Metadata,
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

fn completion_params() -> CompletionCreateParams {
    CompletionCreateParams {
        max_tokens_to_sample: 256,
        model: "claude-opus-4-6".to_owned(),
        prompt: "\n\nHuman: Hello, world!\n\nAssistant:".to_owned(),
        metadata: None,
        stop_sequences: Some(vec!["stop".to_owned()]),
        stream: Some(true),
        temperature: Some(1.0),
        top_k: Some(5),
        top_p: Some(0.7),
        betas: None,
    }
}

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

fn completion_json() -> serde_json::Value {
    serde_json::json!({
        "id": "compl_test",
        "type": "completion",
        "model": "claude-opus-4-6",
        "completion": " Hello",
        "stop_reason": "stop_sequence"
    })
}

#[test]
fn completions_ts_exported_aliases_are_available() {
    assert_same_type::<CompletionCreateParamsBase, CompletionCreateParams>();
    assert_same_type::<CompletionCreateParamsNonStreaming, CompletionCreateParams>();
    assert_same_type::<CompletionCreateParamsStreaming, CompletionCreateParams>();
    assert_same_type::<Metadata, anthropic_sdk::resources::messages::Metadata>();
}

#[tokio::test]
async fn completions_create_posts_complete_and_forces_non_streaming_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let completion = client
        .completions()
        .create(&completion_params())
        .await
        .unwrap();
    assert_eq!(completion.id, "compl_test");

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_tokens_to_sample"], 256);
    assert_eq!(body["model"], "claude-opus-4-6");
}

#[tokio::test]
async fn completions_create_with_response_returns_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_completion")
                .set_body_json(completion_json()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .completions()
        .create_with_response(&completion_params())
        .await
        .unwrap();
    assert_eq!(response.data.id, "compl_test");
    assert_eq!(response.request_id.as_deref(), Some("req_completion"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_completion")
    );
    assert_eq!(
        response.response.json::<serde_json::Value>().unwrap()["id"],
        "compl_test"
    );
}

#[tokio::test]
async fn completions_create_sends_beta_header_and_strips_header_param_from_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(ResponseTemplate::new(200).set_body_json(completion_json()))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut params = completion_params();
    params.betas = Some(vec!["completion-beta".to_owned(), "second-beta".to_owned()]);
    let completion = client.completions().create(&params).await.unwrap();
    assert_eq!(completion.id, "compl_test");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "completion-beta,second-beta"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("betas").is_none());
    assert_eq!(body["stream"], false);
}

#[tokio::test]
async fn completions_create_stream_posts_complete_and_decodes_sse() {
    let server = MockServer::start().await;
    let sse = concat!(
        "event: completion\n",
        "data: {\"id\":\"compl_stream\",\"type\":\"completion\",\"model\":\"claude-opus-4-6\",\"completion\":\" Hi\",\"stop_reason\":null}\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut stream = client
        .completions()
        .create_stream(&completion_params())
        .await
        .unwrap();
    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item.id, "compl_stream");
    assert!(stream.next().await.is_none());

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], true);
}

#[tokio::test]
async fn completions_create_stream_with_response_returns_stream_response_and_request_id_like_ts() {
    let server = MockServer::start().await;
    let sse = concat!(
        "event: completion\n",
        "data: {\"id\":\"compl_stream\",\"type\":\"completion\",\"model\":\"claude-opus-4-6\",\"completion\":\" Hi\",\"stop_reason\":null}\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_completion_stream")
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .completions()
        .create_stream_with_response(&completion_params())
        .await
        .unwrap();
    assert_eq!(
        response.request_id.as_deref(),
        Some("req_completion_stream")
    );
    assert_eq!(
        response.response.header("request-id"),
        Some("req_completion_stream")
    );
    assert!(response.response.body.is_empty());

    let mut stream = response.data;
    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item.id, "compl_stream");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn completions_create_stream_sends_beta_header_and_strips_header_param_from_body() {
    let server = MockServer::start().await;
    let sse = concat!(
        "event: completion\n",
        "data: {\"id\":\"compl_stream\",\"type\":\"completion\",\"model\":\"claude-opus-4-6\",\"completion\":\" Hi\",\"stop_reason\":null}\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/complete"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut params = completion_params();
    params.betas = Some(vec!["completion-beta".to_owned()]);
    let mut stream = client.completions().create_stream(&params).await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "compl_stream");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "completion-beta"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("betas").is_none());
    assert_eq!(body["stream"], true);
}

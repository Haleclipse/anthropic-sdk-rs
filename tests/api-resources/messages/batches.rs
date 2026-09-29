// Integration tests for Message Batches API resources.
//
// Ported from TS SDK: tests/api-resources/messages/batches.test.ts

use std::any::TypeId;

use anthropic_sdk::resources::messages::{
    BatchCreateParams, BatchListParams, BatchRequest, MessageBatchCanceledResult,
    MessageBatchErroredResult, MessageBatchExpiredResult, MessageBatchResult,
    MessageBatchSucceededResult, Request,
};
use anthropic_sdk::{
    Anthropic, ClientOptions, MessageContent, MessageCreateParams, MessageParam, RequestOptions,
};
use futures::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
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

fn message_params() -> MessageCreateParams {
    MessageCreateParams {
        model: "claude-opus-4-6".to_owned(),
        max_tokens: 1024,
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("Hello, world".to_owned()),
        }],
        ..Default::default()
    }
}

fn batch_create_params() -> BatchCreateParams {
    BatchCreateParams {
        requests: vec![BatchRequest {
            custom_id: "my-custom-id-1".to_owned(),
            params: message_params(),
        }],
    }
}

#[test]
fn message_batches_ts_exported_result_aliases_are_available() {
    assert_same_type::<MessageBatchSucceededResult, MessageBatchResult>();
    assert_same_type::<MessageBatchErroredResult, MessageBatchResult>();
    assert_same_type::<MessageBatchCanceledResult, MessageBatchResult>();
    assert_same_type::<MessageBatchExpiredResult, MessageBatchResult>();
    assert_same_type::<Request, BatchRequest>();
}

fn batch_json(id: &str, results_url: Option<&str>) -> serde_json::Value {
    let mut value = serde_json::json!({
        "id": id,
        "type": "message_batch",
        "created_at": "2026-01-01T00:00:00Z",
        "expires_at": "2026-01-02T00:00:00Z",
        "processing_status": "ended",
        "request_counts": {
            "canceled": 0,
            "errored": 0,
            "expired": 0,
            "processing": 0,
            "succeeded": 1
        }
    });
    if let Some(results_url) = results_url {
        value["results_url"] = serde_json::Value::String(results_url.to_owned());
    }
    value
}

fn deleted_batch_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "message_batch_deleted"
    })
}

#[tokio::test]
async fn message_batches_create_posts_typed_message_params() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let batch = client
        .messages()
        .batches()
        .create(&batch_create_params())
        .await
        .unwrap();
    assert_eq!(batch.id, "batch_123");

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["requests"][0]["custom_id"], "my-custom-id-1");
    assert_eq!(body["requests"][0]["params"]["model"], "claude-opus-4-6");
}

#[tokio::test]
async fn message_batches_retrieve_list_cancel_and_delete_use_expected_paths() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [batch_json("batch_123", None)],
            "has_more": false,
            "first_id": "batch_123",
            "last_id": "batch_123"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches/batch_123/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(deleted_batch_json("batch_123")))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieved = client
        .messages()
        .batches()
        .retrieve("batch_123")
        .await
        .unwrap();
    assert_eq!(retrieved.id, "batch_123");
    let listed = client
        .messages()
        .batches()
        .list(Some(&BatchListParams {
            after_id: Some("after".to_owned()),
            before_id: Some("before".to_owned()),
            limit: Some(1),
        }))
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);
    let canceled = client
        .messages()
        .batches()
        .cancel("batch_123")
        .await
        .unwrap();
    assert_eq!(canceled.id, "batch_123");
    let deleted = client
        .messages()
        .batches()
        .delete("batch_123")
        .await
        .unwrap();
    assert_eq!(deleted.id, "batch_123");

    let requests = server.received_requests().await.unwrap();
    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/messages/batches" && request.method == "GET")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn message_batches_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_create")
                .set_body_json(batch_json("batch_create", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_retrieve")
                .set_body_json(batch_json("batch_123", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_list")
                .set_body_json(serde_json::json!({
                    "data": [batch_json("batch_123", None)],
                    "has_more": false,
                    "first_id": "batch_123",
                    "last_id": "batch_123"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches/batch_123/cancel"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_cancel")
                .set_body_json(batch_json("batch_123", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_delete")
                .set_body_json(deleted_batch_json("batch_123")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let batches = client.messages().batches();

    let created = batches
        .create_with_response(&batch_create_params())
        .await
        .unwrap();
    assert_eq!(created.data.id, "batch_create");
    assert_eq!(created.request_id.as_deref(), Some("req_batch_create"));
    assert_eq!(
        created.response.header("request-id"),
        Some("req_batch_create")
    );

    let retrieved = batches.retrieve_with_response("batch_123").await.unwrap();
    assert_eq!(retrieved.data.id, "batch_123");
    assert_eq!(retrieved.request_id.as_deref(), Some("req_batch_retrieve"));

    let listed = batches.list_with_response(None).await.unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_batch_list"));

    let canceled = batches.cancel_with_response("batch_123").await.unwrap();
    assert_eq!(canceled.data.id, "batch_123");
    assert_eq!(canceled.request_id.as_deref(), Some("req_batch_cancel"));

    let deleted = batches.delete_with_response("batch_123").await.unwrap();
    assert_eq!(deleted.data.id, "batch_123");
    assert_eq!(deleted.request_id.as_deref(), Some("req_batch_delete"));
}

#[tokio::test]
async fn message_batches_results_fetches_and_parses_jsonl() {
    let server = MockServer::start().await;
    let results_url = format!("{}/batch_results.jsonl", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(batch_json("batch_123", Some(&results_url))),
        )
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/batch_results.jsonl"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_batch_results")
                .set_body_string(
                    "{\"custom_id\":\"my-custom-id-1\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_123\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-6\",\"content\":[],\"stop_reason\":\"end_turn\",\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}}\n",
                ),
        )
        .expect(3)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut headers = std::collections::HashMap::new();
    headers.insert("x-results-only".to_owned(), Some("yes".to_owned()));
    let options = RequestOptions {
        headers: Some(headers),
        ..Default::default()
    };
    let results = client
        .messages()
        .batches()
        .results_all_with_options("batch_123", Some(&options))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].custom_id, "my-custom-id-1");
    match &results[0].result {
        MessageBatchResult::Succeeded { message } => assert_eq!(message.id, "msg_123"),
        other => panic!("expected succeeded result, got {other:?}"),
    }

    let response = client
        .messages()
        .batches()
        .results_all_with_response("batch_123")
        .await
        .unwrap();
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].custom_id, "my-custom-id-1");
    assert_eq!(response.request_id.as_deref(), Some("req_batch_results"));
    assert_eq!(
        response.response.header("request-id"),
        Some("req_batch_results")
    );
    assert!(String::from_utf8_lossy(&response.response.body).contains("my-custom-id-1"));

    let mut stream_response = client
        .messages()
        .batches()
        .results_with_response("batch_123")
        .await
        .unwrap();
    assert_eq!(
        stream_response.request_id.as_deref(),
        Some("req_batch_results")
    );
    assert_eq!(stream_response.response.body.len(), 0);
    let streamed = stream_response.data.next().await.unwrap().unwrap();
    assert_eq!(streamed.custom_id, "my-custom-id-1");
    assert!(stream_response.data.next().await.is_none());

    let requests = server.received_requests().await.unwrap();
    let retrieve_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/messages/batches/batch_123")
        .unwrap();
    assert!(retrieve_request.headers.get("x-results-only").is_none());
    let results_request = requests
        .iter()
        .find(|request| {
            request.url.path() == "/batch_results.jsonl"
                && request.headers.get("x-results-only").is_some()
        })
        .unwrap();
    assert_eq!(
        results_request.headers.get("x-results-only").unwrap(),
        "yes"
    );
}

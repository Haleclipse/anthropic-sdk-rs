// Integration tests for beta Message Batches API resources.
//
// Ported from TS SDK: tests/api-resources/beta/messages/batches.test.ts

use std::any::TypeId;

use anthropic_sdk::resources::beta::messages::{
    BatchCancelParams, BatchCreateParams, BatchDeleteParams, BatchListParams, BatchResultsParams,
    BatchRetrieveParams, Batches, BetaBatchCancelParams, BetaBatchCreateParams,
    BetaBatchDeleteParams, BetaBatchListParams, BetaBatchRequest, BetaBatchResultsParams,
    BetaBatchRetrieveParams, BetaBatches, BetaMessageBatchCanceledResult,
    BetaMessageBatchErroredResult, BetaMessageBatchExpiredResult, BetaMessageBatchResult,
    BetaMessageBatchSucceededResult, BetaMessageContent, BetaMessageCreateParams, BetaMessageParam,
    Params, Request,
};
use anthropic_sdk::resources::messages::batches::MessageBatchProcessingStatus;
use anthropic_sdk::{Anthropic, ClientOptions};
use futures::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

fn beta_message_params() -> BetaMessageCreateParams {
    BetaMessageCreateParams {
        model: "claude-opus-4-6".to_owned(),
        max_tokens: 1024,
        messages: vec![BetaMessageParam {
            role: "user".to_owned(),
            content: BetaMessageContent::Text("Hello, world".to_owned()),
        }],
        betas: None,
        container: None,
        context_management: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: None,
        output_config: None,
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

#[test]
fn beta_message_batches_ts_exported_aliases_are_available() {
    assert_same_type::<Batches<'static>, BetaBatches<'static>>();
    assert_same_type::<BatchCreateParams, BetaBatchCreateParams>();
    assert_same_type::<BatchRetrieveParams, BetaBatchRetrieveParams>();
    assert_same_type::<BatchListParams, BetaBatchListParams>();
    assert_same_type::<BatchDeleteParams, BetaBatchDeleteParams>();
    assert_same_type::<BatchCancelParams, BetaBatchCancelParams>();
    assert_same_type::<BatchResultsParams, BetaBatchResultsParams>();
    assert_same_type::<BatchResultsParams, BetaBatchRetrieveParams>();
    assert_same_type::<Request, BetaBatchRequest>();
    assert_same_type::<Params, BetaMessageCreateParams>();
    assert_same_type::<BetaMessageBatchSucceededResult, BetaMessageBatchResult>();
    assert_same_type::<BetaMessageBatchErroredResult, BetaMessageBatchResult>();
    assert_same_type::<BetaMessageBatchCanceledResult, BetaMessageBatchResult>();
    assert_same_type::<BetaMessageBatchExpiredResult, BetaMessageBatchResult>();
}

fn beta_batch_create_params() -> BetaBatchCreateParams {
    BetaBatchCreateParams {
        requests: vec![BetaBatchRequest {
            custom_id: "my-custom-id-1".to_owned(),
            params: beta_message_params(),
        }],
        betas: Some(vec!["custom-beta".to_owned()]),
    }
}

fn beta_batch_json(id: &str, results_url: Option<&str>) -> serde_json::Value {
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

fn beta_deleted_batch_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "message_batch_deleted"
    })
}

#[tokio::test]
async fn beta_message_batches_create_posts_beta_endpoint_and_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let batch = client
        .beta()
        .messages()
        .batches()
        .create(&beta_batch_create_params())
        .await
        .unwrap();
    assert_eq!(batch.id, "batch_123");
    assert_eq!(batch.processing_status, MessageBatchProcessingStatus::Ended);

    let requests = server.received_requests().await.unwrap();
    let query: std::collections::HashMap<_, _> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "custom-beta,message-batches-2024-09-24"
    );
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(body.get("betas").is_none());
    assert_eq!(body["requests"][0]["params"]["model"], "claude-opus-4-6");
}

#[tokio::test]
async fn beta_message_batches_retrieve_list_cancel_and_delete_use_beta_paths() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [beta_batch_json("batch_123", None)],
            "has_more": false,
            "first_id": "batch_123",
            "last_id": "batch_123"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches/batch_123/cancel"))
        .respond_with(ResponseTemplate::new(200).set_body_json(beta_batch_json("batch_123", None)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(beta_deleted_batch_json("batch_123")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieve_params = BetaBatchRetrieveParams {
        betas: Some(vec!["custom-beta".to_owned()]),
    };
    let batch = client
        .beta()
        .messages()
        .batches()
        .retrieve("batch_123", Some(&retrieve_params))
        .await
        .unwrap();
    assert_eq!(batch.id, "batch_123");

    let listed = client
        .beta()
        .messages()
        .batches()
        .list(Some(&BetaBatchListParams {
            after_id: Some("after".to_owned()),
            before_id: Some("before".to_owned()),
            limit: Some(1),
            betas: Some(vec!["custom-beta".to_owned()]),
        }))
        .await
        .unwrap();
    assert_eq!(listed.data.len(), 1);

    let canceled = client
        .beta()
        .messages()
        .batches()
        .cancel(
            "batch_123",
            Some(&BetaBatchCancelParams {
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(canceled.id, "batch_123");

    let deleted = client
        .beta()
        .messages()
        .batches()
        .delete(
            "batch_123",
            Some(&BetaBatchDeleteParams {
                betas: Some(vec!["custom-beta".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(deleted.id, "batch_123");

    let requests = server.received_requests().await.unwrap();
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap()
            == "custom-beta,message-batches-2024-09-24"));
    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/messages/batches" && request.method == "GET")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
}

#[tokio::test]
async fn beta_message_batches_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/batches"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_batch_create")
                .set_body_json(beta_batch_json("batch_create", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_batch_retrieve")
                .set_body_json(beta_batch_json("batch_123", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_batch_list")
                .set_body_json(serde_json::json!({
                    "data": [beta_batch_json("batch_123", None)],
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
                .insert_header("request-id", "req_beta_batch_cancel")
                .set_body_json(beta_batch_json("batch_123", None)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_batch_delete")
                .set_body_json(beta_deleted_batch_json("batch_123")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let batches = client.beta().messages().batches();
    let custom_beta = vec!["custom-beta".to_owned()];

    let created = batches
        .create_with_response(&beta_batch_create_params())
        .await
        .unwrap();
    assert_eq!(created.data.id, "batch_create");
    assert_eq!(created.request_id.as_deref(), Some("req_beta_batch_create"));
    assert_eq!(
        created.response.header("request-id"),
        Some("req_beta_batch_create")
    );

    let retrieved = batches
        .retrieve_with_response(
            "batch_123",
            Some(&BetaBatchRetrieveParams {
                betas: Some(custom_beta.clone()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "batch_123");
    assert_eq!(
        retrieved.request_id.as_deref(),
        Some("req_beta_batch_retrieve")
    );

    let listed = batches
        .list_with_response(Some(&BetaBatchListParams {
            after_id: Some("after".to_owned()),
            before_id: Some("before".to_owned()),
            limit: Some(1),
            betas: Some(custom_beta.clone()),
        }))
        .await
        .unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_beta_batch_list"));

    let canceled = batches
        .cancel_with_response(
            "batch_123",
            Some(&BetaBatchCancelParams {
                betas: Some(custom_beta.clone()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(canceled.data.id, "batch_123");
    assert_eq!(
        canceled.request_id.as_deref(),
        Some("req_beta_batch_cancel")
    );

    let deleted = batches
        .delete_with_response(
            "batch_123",
            Some(&BetaBatchDeleteParams {
                betas: Some(custom_beta),
            }),
        )
        .await
        .unwrap();
    assert_eq!(deleted.data.id, "batch_123");
    assert_eq!(deleted.request_id.as_deref(), Some("req_beta_batch_delete"));

    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|request| request
        .url
        .query_pairs()
        .any(|(k, v)| k == "beta" && v == "true")));
}

#[tokio::test]
async fn beta_message_batches_results_fetches_and_parses_jsonl_with_beta_header() {
    let server = MockServer::start().await;
    let results_url = format!("{}/beta_batch_results.jsonl", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/messages/batches/batch_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(beta_batch_json("batch_123", Some(&results_url))),
        )
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/beta_batch_results.jsonl"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_batch_results")
                .set_body_string(
                    "{\"custom_id\":\"my-custom-id-1\",\"result\":{\"type\":\"succeeded\",\"message\":{\"id\":\"msg_123\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-opus-4-6\",\"content\":[],\"stop_reason\":\"end_turn\",\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}}\n",
                ),
        )
        .expect(3)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaBatchRetrieveParams {
        betas: Some(vec!["custom-beta".to_owned()]),
    };
    let results = client
        .beta()
        .messages()
        .batches()
        .results_all("batch_123", Some(&params))
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].custom_id, "my-custom-id-1");
    match &results[0].result {
        BetaMessageBatchResult::Succeeded { message } => assert_eq!(message.id, "msg_123"),
        other => panic!("expected succeeded result, got {other:?}"),
    }

    let response = client
        .beta()
        .messages()
        .batches()
        .results_all_with_response("batch_123", Some(&params))
        .await
        .unwrap();
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].custom_id, "my-custom-id-1");
    assert_eq!(
        response.request_id.as_deref(),
        Some("req_beta_batch_results")
    );
    assert_eq!(
        response.response.header("request-id"),
        Some("req_beta_batch_results")
    );
    assert!(String::from_utf8_lossy(&response.response.body).contains("my-custom-id-1"));

    let mut stream_response = client
        .beta()
        .messages()
        .batches()
        .results_with_response("batch_123", Some(&params))
        .await
        .unwrap();
    assert_eq!(
        stream_response.request_id.as_deref(),
        Some("req_beta_batch_results")
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
    assert_eq!(
        retrieve_request.headers.get("anthropic-beta").unwrap(),
        "message-batches-2024-09-24"
    );
    let results_request = requests
        .iter()
        .find(|request| request.url.path() == "/beta_batch_results.jsonl")
        .unwrap();
    assert_eq!(
        results_request.headers.get("anthropic-beta").unwrap(),
        "custom-beta,message-batches-2024-09-24"
    );
}

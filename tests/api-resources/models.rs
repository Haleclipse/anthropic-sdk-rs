// Integration tests for Models API resources.
//
// Ported from TS SDK: tests/api-resources/models.test.ts

use anthropic_sdk::resources::models::{ModelListParams, ModelRetrieveParams};
use anthropic_sdk::{Anthropic, ClientOptions, RequestOptions};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_client(server_url: &str) -> Anthropic {
    Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

fn model_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "type": "model",
        "created_at": "2026-01-01T00:00:00Z",
        "display_name": "Claude Test"
    })
}

#[tokio::test]
async fn models_retrieve_encodes_path_and_sends_beta_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/model%2Fwith%20space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json("model/with space")))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let model = client
        .models()
        .retrieve(
            "model/with space",
            Some(&ModelRetrieveParams {
                betas: Some(vec!["beta-flag".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(model.id, "model/with space");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn models_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/model_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_model")
                .set_body_json(model_json("model_123")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_models")
                .set_body_json(serde_json::json!({
                    "data": [model_json("claude-test")],
                    "has_more": false,
                    "first_id": "claude-test",
                    "last_id": "claude-test"
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let retrieved = client
        .models()
        .retrieve_with_response("model_123", None)
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "model_123");
    assert_eq!(retrieved.request_id.as_deref(), Some("req_model"));
    assert_eq!(retrieved.response.header("request-id"), Some("req_model"));

    let listed = client.models().list_with_response(None).await.unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_models"));
    assert_eq!(
        listed.response.json::<serde_json::Value>().unwrap()["data"][0]["id"],
        "claude-test"
    );
}

#[tokio::test]
async fn models_list_sends_query_and_beta_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [model_json("claude-test")],
            "has_more": false,
            "first_id": "claude-test",
            "last_id": "claude-test"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let page = client
        .models()
        .list(Some(&ModelListParams {
            before_id: Some("before".to_owned()),
            after_id: Some("after".to_owned()),
            limit: Some(1),
            betas: Some(vec!["beta-flag".to_owned()]),
        }))
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);

    let requests = server.received_requests().await.unwrap();
    let query: std::collections::HashMap<_, _> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn models_request_options_path_override_applies_to_retrieve_and_list() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models-retrieve-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json("claude-override")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models-list-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [model_json("claude-test")],
            "has_more": false,
            "first_id": "claude-test",
            "last_id": "claude-test"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());

    let retrieve_options = RequestOptions {
        path: Some("/v1/models-retrieve-override".to_owned()),
        ..Default::default()
    };
    let model = client
        .models()
        .retrieve_with_options(
            "ignored_model_id",
            Some(&ModelRetrieveParams {
                betas: Some(vec!["beta-flag".to_owned()]),
            }),
            Some(&retrieve_options),
        )
        .await
        .unwrap();
    assert_eq!(model.id, "claude-override");

    let list_options = RequestOptions {
        path: Some("/v1/models-list-override".to_owned()),
        ..Default::default()
    };
    let page = client
        .models()
        .list_with_options(
            Some(&ModelListParams {
                before_id: Some("before".to_owned()),
                after_id: Some("after".to_owned()),
                limit: Some(1),
                betas: Some(vec!["beta-flag".to_owned()]),
            }),
            Some(&list_options),
        )
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.headers.get("anthropic-beta").unwrap() == "beta-flag"));

    let list_request = requests
        .iter()
        .find(|request| request.url.path() == "/v1/models-list-override")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
}

// Integration tests for beta Models API resources.
//
// Ported from TS SDK: tests/api-resources/beta/models.test.ts

use std::any::TypeId;

use anthropic_sdk::resources::beta::models::{
    BetaModelListParams, BetaModelRetrieveParams, BetaModels, ModelListParams, ModelRetrieveParams,
    Models,
};
use anthropic_sdk::{Anthropic, ClientOptions, RequestOptions};
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

#[test]
fn beta_models_ts_exported_aliases_are_available() {
    assert_same_type::<Models<'static>, BetaModels<'static>>();
    assert_same_type::<ModelRetrieveParams, BetaModelRetrieveParams>();
    assert_same_type::<ModelListParams, BetaModelListParams>();
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
async fn beta_models_retrieve_sends_beta_query_and_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/model%2Fwith%20space"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json("model/with space")))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let model = client
        .beta()
        .models()
        .retrieve(
            "model/with space",
            Some(&BetaModelRetrieveParams {
                betas: Some(vec!["beta-flag".to_owned()]),
            }),
        )
        .await
        .unwrap();
    assert_eq!(model.id, "model/with space");

    let requests = server.received_requests().await.unwrap();
    let query: std::collections::HashMap<_, _> = requests[0]
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn beta_models_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models/model_123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_model")
                .set_body_json(model_json("model_123")),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_beta_models")
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
        .beta()
        .models()
        .retrieve_with_response("model_123", None)
        .await
        .unwrap();
    assert_eq!(retrieved.data.id, "model_123");
    assert_eq!(retrieved.request_id.as_deref(), Some("req_beta_model"));
    assert_eq!(
        retrieved.response.header("request-id"),
        Some("req_beta_model")
    );

    let listed = client
        .beta()
        .models()
        .list_with_response(None)
        .await
        .unwrap();
    assert_eq!(listed.data.data.len(), 1);
    assert_eq!(listed.request_id.as_deref(), Some("req_beta_models"));
    assert_eq!(
        listed.response.json::<serde_json::Value>().unwrap()["data"][0]["id"],
        "claude-test"
    );

    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().all(|request| request
        .url
        .query_pairs()
        .any(|(key, value)| key == "beta" && value == "true")));
}

#[tokio::test]
async fn beta_models_list_sends_query_and_beta_header() {
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
        .beta()
        .models()
        .list(Some(&BetaModelListParams {
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
    assert_eq!(query.get("beta").map(String::as_str), Some("true"));
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn beta_models_request_options_path_override_applies_to_retrieve_and_list() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/beta-models-retrieve-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(model_json("claude-override")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/beta-models-list-override"))
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
        path: Some("/v1/beta-models-retrieve-override".to_owned()),
        ..Default::default()
    };
    let model = client
        .beta()
        .models()
        .retrieve_with_options(
            "ignored_model_id",
            Some(&BetaModelRetrieveParams {
                betas: Some(vec!["beta-flag".to_owned()]),
            }),
            Some(&retrieve_options),
        )
        .await
        .unwrap();
    assert_eq!(model.id, "claude-override");

    let list_options = RequestOptions {
        path: Some("/v1/beta-models-list-override".to_owned()),
        ..Default::default()
    };
    let page = client
        .beta()
        .models()
        .list_with_options(
            Some(&BetaModelListParams {
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
        .find(|request| request.url.path() == "/v1/beta-models-list-override")
        .unwrap();
    let query: std::collections::HashMap<_, _> = list_request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert!(!query.contains_key("beta"));
    assert_eq!(query.get("before_id").map(String::as_str), Some("before"));
    assert_eq!(query.get("after_id").map(String::as_str), Some("after"));
    assert_eq!(query.get("limit").map(String::as_str), Some("1"));
}

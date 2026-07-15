// Integration tests for TS-style path parameter encoding.

use anthropic_sdk::{Anthropic, ClientOptions};
use wiremock::matchers::method;
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

#[tokio::test]
async fn resource_path_params_are_percent_encoded_like_ts_path_tag() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "model/with space",
            "created_at": "2026-01-01T00:00:00Z",
            "display_name": "Model With Space",
            "type": "model"
        })))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let model = client
        .models()
        .retrieve("model/with space", None)
        .await
        .unwrap();
    assert_eq!(model.id, "model/with space");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/v1/models/model%2Fwith%20space");
}

#[tokio::test]
async fn dot_segments_are_rejected_as_unsafe_path_params() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri());

    let err = client.models().retrieve("..", None).await.unwrap_err();
    assert!(format!("{err}").contains("can't be safely passed as a path parameter"));
}

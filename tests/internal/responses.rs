// Mirrors TS SDK tests/responses.test.ts cases that affect Rust response parsing.

use anthropic_sdk::core::pagination::Page;
use anthropic_sdk::internal::parse::parse_json_response;
use anthropic_sdk::{Anthropic, ApiError, ClientOptions};
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

#[tokio::test]
async fn text_response_parses_as_string_like_ts_default_parse_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/text"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/plain")
                .set_body_string("hello world"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let text: String = client.get("/v1/text", None, None).await.unwrap();
    assert_eq!(text, "hello world");
}

#[tokio::test]
async fn text_response_can_parse_as_json_value_string_like_ts_default_parse_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/text-value"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/plain")
                .set_body_string("hello world"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let value: serde_json::Value = client.get("/v1/text-value", None, None).await.unwrap();
    assert_eq!(value, serde_json::Value::String("hello world".to_owned()));
}

#[tokio::test]
async fn empty_json_response_parses_as_null_like_ts_default_parse_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/empty-json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .insert_header("content-length", "0"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let value: serde_json::Value = client.get("/v1/empty-json", None, None).await.unwrap();
    assert_eq!(value, serde_json::Value::Null);
}

#[tokio::test]
async fn no_content_response_parses_as_null_like_ts_default_parse_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/no-content"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let value: serde_json::Value = client.get("/v1/no-content", None, None).await.unwrap();
    assert_eq!(value, serde_json::Value::Null);
}

#[tokio::test]
async fn status_error_empty_and_falsy_json_bodies_match_ts_api_error_make_message() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/empty-error"))
        .respond_with(ResponseTemplate::new(400))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/false-error"))
        .respond_with(
            ResponseTemplate::new(400)
                .insert_header("content-type", "application/json")
                .set_body_string("false"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());

    let empty_err = client
        .get::<serde_json::Value>("/v1/empty-error", None, None)
        .await
        .unwrap_err();
    assert!(matches!(empty_err, ApiError::BadRequest { .. }));
    assert_eq!(empty_err.to_string(), "400 status code (no body)");

    let false_err = client
        .get::<serde_json::Value>("/v1/false-error", None, None)
        .await
        .unwrap_err();
    assert!(matches!(false_err, ApiError::BadRequest { .. }));
    assert_eq!(false_err.to_string(), "400 false");
}

#[tokio::test]
async fn internal_parse_adds_request_id_to_json_object_like_ts_add_request_id() {
    let response: reqwest::Response = http::Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("request-id", "req_internal")
        .body(r#"{"id":"bar"}"#)
        .unwrap()
        .into();

    let value: serde_json::Value = parse_json_response(response).await.unwrap();
    assert_eq!(value["id"], "bar");
    assert_eq!(value["_request_id"], "req_internal");
}

#[tokio::test]
async fn array_response_does_not_receive_request_id_like_ts_array_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/array"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_array")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!([{"foo": "bar"}])),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let value: Vec<serde_json::Value> = client.get("/v1/array", None, None).await.unwrap();
    assert_eq!(value, vec![serde_json::json!({"foo": "bar"})]);
}

#[tokio::test]
async fn page_response_does_not_expose_request_id_like_ts_page_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/page"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_page")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "data": [{"foo": "bar"}],
                    "has_more": false,
                    "first_id": null,
                    "last_id": null
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let page: Page<serde_json::Value> = client.get("/v1/page", None, None).await.unwrap();
    assert_eq!(page.data, vec![serde_json::json!({"foo": "bar"})]);
    assert!(!page.has_next_page());
}

#[tokio::test]
async fn internal_parse_does_not_add_request_id_to_json_arrays_like_ts_add_request_id() {
    let response: reqwest::Response = http::Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .header("request-id", "req_internal")
        .body(r#"[{"id":"bar"}]"#)
        .unwrap()
        .into();

    let value: serde_json::Value = parse_json_response(response).await.unwrap();
    assert_eq!(value, serde_json::json!([{"id": "bar"}]));
}

#[tokio::test]
async fn json_response_still_parses_by_content_type() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let value: serde_json::Value = client.get("/v1/json", None, None).await.unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));
}

#[tokio::test]
async fn raw_response_helper_matches_ts_as_response_shape() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/raw"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_raw")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"id": "bar"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .get_raw_response("/v1/raw", None, None, None)
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert!(response.url.ends_with("/v1/raw"));
    assert_eq!(response.header("Request-ID"), Some("req_raw"));
    assert_eq!(response.request_id(), Some("req_raw"));
    assert_eq!(
        response.json::<serde_json::Value>().unwrap(),
        serde_json::json!({"id": "bar"})
    );
    assert_eq!(response.text().unwrap(), r#"{"id":"bar"}"#);
}

#[tokio::test]
async fn put_and_patch_raw_with_response_helpers_match_ts_as_response_with_response() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/put-raw"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_put_raw")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"method": "put"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v1/put-with-response"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_put_with")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"method": "put"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1/patch-raw"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_patch_raw")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"method": "patch"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1/patch-with-response"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_patch_with")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"method": "patch"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let put_raw = client
        .put_raw_response(
            "/v1/put-raw",
            &serde_json::json!({"body": true}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(put_raw.request_id(), Some("req_put_raw"));
    assert_eq!(
        put_raw.json::<serde_json::Value>().unwrap()["method"],
        "put"
    );

    let put_with = client
        .put_with_response::<_, serde_json::Value>(
            "/v1/put-with-response",
            &serde_json::json!({"body": true}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(put_with.request_id.as_deref(), Some("req_put_with"));
    assert_eq!(put_with.data["_request_id"], "req_put_with");

    let patch_raw = client
        .patch_raw_response(
            "/v1/patch-raw",
            &serde_json::json!({"body": true}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(patch_raw.request_id(), Some("req_patch_raw"));
    assert_eq!(
        patch_raw.json::<serde_json::Value>().unwrap()["method"],
        "patch"
    );

    let patch_with = client
        .patch_with_response::<_, serde_json::Value>(
            "/v1/patch-with-response",
            &serde_json::json!({"body": true}),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(patch_with.request_id.as_deref(), Some("req_patch_with"));
    assert_eq!(patch_with.data["_request_id"], "req_patch_with");
}

#[tokio::test]
async fn with_response_helper_returns_data_response_and_request_id_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/with-response"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_with")
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"id": "bar"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let response = client
        .get_with_response::<serde_json::Value>("/v1/with-response", None, None, None)
        .await
        .unwrap();

    assert_eq!(response.request_id.as_deref(), Some("req_with"));
    assert_eq!(response.response.header("request-id"), Some("req_with"));
    assert_eq!(response.data["id"], "bar");
    assert_eq!(response.data["_request_id"], "req_with");
    // The raw buffered body remains the server body; request-id injection only
    // affects parsed object data, matching TS addRequestID non-enumerability in spirit.
    assert_eq!(
        response.response.json::<serde_json::Value>().unwrap(),
        serde_json::json!({"id": "bar"})
    );
}

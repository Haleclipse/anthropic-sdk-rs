// Integration tests for RequestOptions parity: per-request method/path,
// headers, query, timeout header, max retries, and abort/cancel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anthropic_sdk::{
    AbortSignal, Anthropic, ClientOptions, HttpMiddleware, LogLevel, RequestOptions, SdkLogger,
};
use futures::future::BoxFuture;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_client(server_url: &str, max_retries: u32) -> Anthropic {
    let mut default_headers = HashMap::new();
    default_headers.insert("x-test".to_owned(), Some("default".to_owned()));
    default_headers.insert("x-remove".to_owned(), Some("remove-me".to_owned()));

    let mut default_query = HashMap::new();
    default_query.insert("keep".to_owned(), Some("default".to_owned()));
    default_query.insert("remove".to_owned(), Some("default".to_owned()));

    Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server_url.to_owned()),
        max_retries: Some(max_retries),
        default_headers: Some(default_headers),
        default_query: Some(default_query),
        ..Default::default()
    })
    .expect("client creation should succeed")
}

#[tokio::test]
async fn client_options_custom_http_client_is_used_and_inherited_like_ts_custom_fetch() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/custom-http-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(2)
        .mount(&server)
        .await;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-custom-http-client",
        reqwest::header::HeaderValue::from_static("yes"),
    );
    let http_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        http_client: Some(http_client),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let value: serde_json::Value = client
        .get("/v1/custom-http-client", None, None)
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let child = client
        .withOptions(ClientOptions {
            default_headers: Some(HashMap::from([(
                "x-child-default".to_owned(),
                Some("preserved".to_owned()),
            )])),
            ..Default::default()
        })
        .unwrap();
    let value: serde_json::Value = child
        .get("/v1/custom-http-client", None, None)
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| {
        request
            .headers
            .get("x-custom-http-client")
            .and_then(|value| value.to_str().ok())
            == Some("yes")
    }));
    assert_eq!(
        requests[1]
            .headers
            .get("x-child-default")
            .and_then(|value| value.to_str().ok()),
        Some("preserved")
    );
}

#[tokio::test]
async fn request_retries_on_timeout_like_ts_index_retry_on_timeout_case() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let responder_attempts = Arc::clone(&attempts);

    Mock::given(method("GET"))
        .and(path("/v1/retry-timeout"))
        .respond_with(move |_request: &wiremock::Request| {
            let attempt = responder_attempts.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt == 1 {
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(500))
                    .set_body_json(serde_json::json!({"too_late": true}))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true}))
            }
        })
        .expect(2)
        .mount(&server)
        .await;

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        timeout: Some(150),
        max_retries: Some(1),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let value: serde_json::Value = client
        .get("/v1/retry-timeout", None, None)
        .await
        .expect("second request should succeed after retrying timeout");
    assert_eq!(value, serde_json::json!({"ok": true}));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]
            .headers
            .get("x-stainless-retry-count")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
}

#[tokio::test]
async fn request_options_http_client_overrides_client_http_client_like_per_request_fetch_options() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/per-request-http-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-per-request-http-client",
        reqwest::header::HeaderValue::from_static("yes"),
    );
    let per_request_http_client = reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .unwrap();

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed");
    let options = RequestOptions {
        http_client: Some(per_request_http_client),
        ..Default::default()
    };

    let value: serde_json::Value = client
        .get_with_options("/v1/per-request-http-client", None, None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("x-per-request-http-client")
            .and_then(|value| value.to_str().ok()),
        Some("yes")
    );
}

type CapturedLogDetails = (LogLevel, String, serde_json::Map<String, serde_json::Value>);

#[derive(Default)]
struct CaptureLogger {
    messages: Mutex<Vec<(LogLevel, String)>>,
    details: Mutex<Vec<CapturedLogDetails>>,
}

impl SdkLogger for CaptureLogger {
    fn log(&self, level: LogLevel, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((level, message.to_owned()));
    }

    fn log_with_details(
        &self,
        level: LogLevel,
        message: &str,
        details: &serde_json::Map<String, serde_json::Value>,
    ) {
        self.log(level, message);
        self.details
            .lock()
            .unwrap()
            .push((level, message.to_owned(), details.clone()));
    }
}

#[derive(Clone)]
struct AddHeaderMiddleware {
    name: reqwest::header::HeaderName,
    value: reqwest::header::HeaderValue,
    before_count: Arc<AtomicUsize>,
    response_count: Arc<AtomicUsize>,
}

impl AddHeaderMiddleware {
    fn new(name: &'static str, value: &'static str) -> Self {
        Self {
            name: reqwest::header::HeaderName::from_static(name),
            value: reqwest::header::HeaderValue::from_static(value),
            before_count: Arc::new(AtomicUsize::new(0)),
            response_count: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl HttpMiddleware for AddHeaderMiddleware {
    fn before_request<'a>(
        &'a self,
        request: &'a mut reqwest::Request,
    ) -> BoxFuture<'a, Result<(), anthropic_sdk::ApiError>> {
        Box::pin(async move {
            self.before_count.fetch_add(1, Ordering::SeqCst);
            request
                .headers_mut()
                .insert(self.name.clone(), self.value.clone());
            Ok(())
        })
    }

    fn on_response<'a>(
        &'a self,
        _response: &'a reqwest::Response,
    ) -> BoxFuture<'a, Result<(), anthropic_sdk::ApiError>> {
        Box::pin(async move {
            self.response_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

#[test]
fn client_options_log_level_env_default_invalid_and_override_match_ts() {
    let original = std::env::var_os("ANTHROPIC_LOG");

    std::env::remove_var("ANTHROPIC_LOG");
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        ..Default::default()
    })
    .expect("client creation should succeed");
    assert_eq!(client.log_level(), LogLevel::Warn);

    std::env::set_var("ANTHROPIC_LOG", "debug");
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        ..Default::default()
    })
    .expect("client creation should succeed");
    assert_eq!(client.log_level(), LogLevel::Debug);

    std::env::set_var("ANTHROPIC_LOG", "not a log level");
    let logger = Arc::new(CaptureLogger::default());
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        logger: Some(logger.clone()),
        ..Default::default()
    })
    .expect("client creation should succeed");
    assert_eq!(client.log_level(), LogLevel::Warn);
    let messages = logger.messages.lock().unwrap();
    assert!(messages.iter().any(|(level, message)| {
        *level == LogLevel::Warn
            && message == "process.env['ANTHROPIC_LOG'] was set to \"not a log level\", expected one of [\"off\",\"error\",\"warn\",\"info\",\"debug\"]"
    }));
    drop(messages);

    let logger = Arc::new(CaptureLogger::default());
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        log_level: Some(LogLevel::Off),
        logger: Some(logger.clone()),
        ..Default::default()
    })
    .expect("client creation should succeed");
    assert_eq!(client.log_level(), LogLevel::Off);
    assert!(logger.messages.lock().unwrap().is_empty());

    match original {
        Some(value) => std::env::set_var("ANTHROPIC_LOG", value),
        None => std::env::remove_var("ANTHROPIC_LOG"),
    }
}

#[tokio::test]
async fn client_options_logger_and_log_level_capture_request_lifecycle_like_ts_logger() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/logging"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let logger = Arc::new(CaptureLogger::default());
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        log_level: Some(LogLevel::Info),
        logger: Some(logger.clone()),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let value: serde_json::Value = client.get("/v1/logging", None, None).await.unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let messages = logger.messages.lock().unwrap();
    assert!(messages.iter().any(|(level, message)| {
        *level == LogLevel::Info
            && message.contains("/v1/logging")
            && message.contains("succeeded with status 200")
    }));
    assert!(
        messages.iter().all(|(level, _)| *level != LogLevel::Debug),
        "info level should suppress debug request-start logs: {messages:?}"
    );
}

#[tokio::test]
async fn client_options_debug_logging_redacts_sensitive_headers_like_ts_format_request_details() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/logging-redaction"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let logger = Arc::new(CaptureLogger::default());
    let client = Anthropic::new(ClientOptions {
        api_key: Some("secret-api-key".to_owned()),
        auth_token: Some("secret-auth-token".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        log_level: Some(LogLevel::Debug),
        logger: Some(logger.clone()),
        default_headers: Some(HashMap::from([(
            "cookie".to_owned(),
            Some("secret-cookie".to_owned()),
        )])),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let value: serde_json::Value = client
        .get("/v1/logging-redaction", None, None)
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let messages = logger.messages.lock().unwrap();
    let debug = messages
        .iter()
        .find(|(level, message)| *level == LogLevel::Debug && message.contains("sending request"))
        .expect("debug request-start log should be captured")
        .1
        .clone();
    assert!(debug.contains("[log_"), "debug log was: {debug}");
    assert!(debug.contains("x-api-key"), "debug log was: {debug}");
    assert!(debug.contains("authorization"), "debug log was: {debug}");
    assert!(debug.contains("cookie"), "debug log was: {debug}");
    assert!(debug.contains("***"), "debug log was: {debug}");
    assert!(!debug.contains("secret-api-key"), "debug log was: {debug}");
    assert!(
        !debug.contains("secret-auth-token"),
        "debug log was: {debug}"
    );
    assert!(!debug.contains("secret-cookie"), "debug log was: {debug}");
    drop(messages);

    let details = logger.details.lock().unwrap();
    let (_, _, request_details) = details
        .iter()
        .find(|(level, message, _)| {
            *level == LogLevel::Debug && message.contains("sending request")
        })
        .expect("structured request-start details should be captured");
    assert_eq!(request_details["method"], "get");
    assert!(request_details["url"]
        .as_str()
        .is_some_and(|url| url.ends_with("/v1/logging-redaction")));
    assert_eq!(request_details["headers"]["x-api-key"], "***");
    assert_eq!(request_details["headers"]["authorization"], "***");
    assert_eq!(request_details["headers"]["cookie"], "***");

    let (_, _, response_details) = details
        .iter()
        .find(|(level, message, _)| *level == LogLevel::Debug && message.contains("response start"))
        .expect("structured response-start details should be captured");
    assert_eq!(response_details["status"], 200);
    assert!(response_details["url"]
        .as_str()
        .is_some_and(|url| url.ends_with("/v1/logging-redaction")));
    assert!(response_details["durationMs"].is_number());
}

#[tokio::test]
async fn client_options_retry_logging_includes_request_log_id_and_retry_of_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/logging-retry"))
        .respond_with(
            ResponseTemplate::new(500)
                .insert_header("retry-after-ms", "0")
                .set_body_json(serde_json::json!({"error": {"message": "try again"}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/logging-retry"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_retry")
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let logger = Arc::new(CaptureLogger::default());
    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(1),
        log_level: Some(LogLevel::Debug),
        logger: Some(logger.clone()),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let value: serde_json::Value = client.get("/v1/logging-retry", None, None).await.unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["_request_id"], "req_retry");

    let messages = logger.messages.lock().unwrap();
    let request_starts: Vec<_> = messages
        .iter()
        .filter(|(level, message)| *level == LogLevel::Debug && message.contains("sending request"))
        .map(|(_, message)| message.clone())
        .collect();
    assert_eq!(request_starts.len(), 2, "logs were: {messages:?}");

    let first_id = request_starts[0]
        .split(']')
        .next()
        .and_then(|part| part.strip_prefix('['))
        .expect("request log id should be bracketed")
        .to_owned();
    assert!(first_id.starts_with("log_"), "first id was: {first_id}");
    assert!(
        request_starts[1].contains(&format!("retryOf: {first_id}")),
        "second request log should reference first id; logs were: {request_starts:?}"
    );
    assert!(messages.iter().any(|(level, message)| {
        *level == LogLevel::Info
            && message.contains(&format!("retryOf: {first_id}"))
            && message.contains("request-id: \"req_retry\"")
            && message.contains("succeeded with status 200")
    }));
}

#[tokio::test]
async fn core_post_multipart_helper_uses_common_request_pipeline_like_ts_multipart_form_request_options(
) {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/core-multipart"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_core_multipart")
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let value: serde_json::Value = client
        .post_multipart(
            "/v1/core-multipart",
            || Ok(reqwest::multipart::Form::new().text("purpose", "eval")),
            None,
        )
        .await
        .unwrap();

    assert_eq!(value["ok"], true);
    assert_eq!(value["_request_id"], "req_core_multipart");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let content_type = requests[0]
        .headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.starts_with("multipart/form-data; boundary="),
        "content-type was {content_type}"
    );
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"purpose\""), "body was {body}");
    assert!(body.contains("eval"), "body was {body}");
}

#[tokio::test]
async fn request_options_override_headers_query_and_timeout_header() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);

    let mut resource_query = HashMap::new();
    resource_query.insert("resource".to_owned(), Some("1".to_owned()));

    let mut option_query = HashMap::new();
    option_query.insert("opt".to_owned(), Some("2".to_owned()));
    option_query.insert("remove".to_owned(), None);

    let mut option_headers = HashMap::new();
    option_headers.insert("x-test".to_owned(), Some("request".to_owned()));
    option_headers.insert("x-remove".to_owned(), None);

    let options = RequestOptions {
        headers: Some(option_headers),
        query: Some(option_query),
        timeout: Some(Duration::from_millis(1500)),
        ..Default::default()
    };

    let value: serde_json::Value = client
        .get_with_options("/v1/test", Some(&resource_query), None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"ok": true}));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];

    let pairs: HashMap<_, _> = request.url.query_pairs().into_owned().collect();
    assert_eq!(pairs.get("keep").map(String::as_str), Some("default"));
    assert_eq!(pairs.get("resource").map(String::as_str), Some("1"));
    assert_eq!(pairs.get("opt").map(String::as_str), Some("2"));
    assert!(!pairs.contains_key("remove"));

    assert_eq!(
        request.headers.get("x-test").and_then(|v| v.to_str().ok()),
        Some("request")
    );
    assert!(request.headers.get("x-remove").is_none());
    assert_eq!(
        request
            .headers
            .get("x-stainless-timeout")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
}

#[tokio::test]
async fn request_options_default_base_url_used_only_when_client_base_is_default() {
    let default_base_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/default-base"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"base": "default"})),
        )
        .expect(1)
        .mount(&default_base_server)
        .await;

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        max_retries: Some(0),
        ..Default::default()
    })
    .expect("client creation should succeed");
    let options = RequestOptions {
        default_base_url: Some(default_base_server.uri()),
        ..Default::default()
    };

    let value: serde_json::Value = client
        .get_with_options("/v1/default-base", None, None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"base": "default"}));

    let explicit_base_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/explicit-base"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"base": "explicit"})),
        )
        .expect(1)
        .mount(&explicit_base_server)
        .await;

    let ignored_default_base_server = MockServer::start().await;
    let explicit_client = mock_client(&explicit_base_server.uri(), 0);
    let options = RequestOptions {
        default_base_url: Some(ignored_default_base_server.uri()),
        ..Default::default()
    };

    let value: serde_json::Value = explicit_client
        .get_with_options("/v1/explicit-base", None, None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value, serde_json::json!({"base": "explicit"}));
    assert!(ignored_default_base_server
        .received_requests()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn request_options_apply_to_put_and_patch_helpers() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/put"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"method": "put"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1/patch"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"method": "patch"})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let mut headers = HashMap::new();
    headers.insert("x-method-option".to_owned(), Some("yes".to_owned()));
    let options = RequestOptions {
        headers: Some(headers),
        ..Default::default()
    };

    let put: serde_json::Value = client
        .put_with_options(
            "/v1/put",
            &serde_json::json!({"body": true}),
            None,
            Some(&options),
        )
        .await
        .unwrap();
    let patch: serde_json::Value = client
        .patch_with_options(
            "/v1/patch",
            &serde_json::json!({"body": true}),
            None,
            Some(&options),
        )
        .await
        .unwrap();

    assert_eq!(put, serde_json::json!({"method": "put"}));
    assert_eq!(patch, serde_json::json!({"method": "patch"}));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(
            request
                .headers
                .get("x-method-option")
                .and_then(|value| value.to_str().ok()),
            Some("yes")
        );
    }
}

#[tokio::test]
async fn request_options_path_and_method_override_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "path": "override"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let options = RequestOptions {
        method: Some(reqwest::Method::POST),
        path: Some("/v1/override".to_owned()),
        ..Default::default()
    };

    let value: serde_json::Value = client
        .get_with_options("/v1/original", None, None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value["path"], "override");
}

#[tokio::test]
async fn request_options_body_overrides_generated_json_body_like_ts_spread_order() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/body-override"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let options = RequestOptions {
        body: Some(serde_json::json!({"override": true})),
        ..Default::default()
    };

    let value: serde_json::Value = client
        .post_with_options(
            "/v1/body-override",
            &serde_json::json!({"generated": true}),
            None,
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(value["ok"], true);

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body, serde_json::json!({"override": true}));
}

#[tokio::test]
async fn request_options_falsy_body_suppresses_generated_body_like_ts_build_body() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/body-null"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/body-false"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);

    let null_body = RequestOptions {
        body: Some(serde_json::Value::Null),
        ..Default::default()
    };
    let _: serde_json::Value = client
        .post_with_options(
            "/v1/body-null",
            &serde_json::json!({"generated": true}),
            None,
            Some(&null_body),
        )
        .await
        .unwrap();

    let false_body = RequestOptions {
        body: Some(serde_json::Value::Bool(false)),
        ..Default::default()
    };
    let _: serde_json::Value = client
        .post_with_options(
            "/v1/body-false",
            &serde_json::json!({"generated": true}),
            None,
            Some(&false_body),
        )
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert!(request.body.is_empty(), "body was {:?}", request.body);
        assert!(
            request.headers.get("content-type").is_none(),
            "content-type was {:?}",
            request.headers.get("content-type")
        );
    }
}

#[tokio::test]
async fn request_options_raw_body_overrides_generated_json_like_go_with_request_body() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/raw-body"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let options = RequestOptions::default().with_request_body(
        "application/custom+json",
        bytes::Bytes::from_static(b"raw payload"),
    );

    let value: serde_json::Value = client
        .post_with_options(
            "/v1/raw-body",
            &serde_json::json!({"generated": true}),
            None,
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(value["ok"], true);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(&requests[0].body, b"raw payload");
    assert_eq!(
        requests[0]
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/custom+json")
    );
}

#[tokio::test]
async fn request_options_json_patches_match_go_json_set_and_del_escape_hatches() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/json-patch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let options = RequestOptions::default()
        .with_json_set("metadata.trace_id", "trace_123")
        .with_json_set("items.0.name", "first")
        .with_json_delete("remove_me");

    let _: serde_json::Value = client
        .post_with_options(
            "/v1/json-patch",
            &serde_json::json!({"generated": true, "remove_me": "gone"}),
            None,
            Some(&options),
        )
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["generated"], true);
    assert_eq!(body["metadata"]["trace_id"], "trace_123");
    assert_eq!(body["items"][0]["name"], "first");
    assert!(body.get("remove_me").is_none());
}

#[tokio::test]
async fn client_and_request_middlewares_mutate_requests_and_observe_responses_like_go() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/middleware"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client_middleware = AddHeaderMiddleware::new("x-client-middleware", "client");
    let request_middleware = AddHeaderMiddleware::new("x-request-middleware", "request");

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(0),
        middlewares: vec![Arc::new(client_middleware.clone()) as Arc<dyn HttpMiddleware>],
        ..Default::default()
    })
    .expect("client creation should succeed");
    let options = RequestOptions::default().with_middleware(request_middleware.clone());

    let value: serde_json::Value = client
        .get_with_options("/v1/middleware", None, None, Some(&options))
        .await
        .unwrap();
    assert_eq!(value["ok"], true);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-client-middleware")
            .and_then(|value| value.to_str().ok()),
        Some("client")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-request-middleware")
            .and_then(|value| value.to_str().ok()),
        Some("request")
    );
    assert_eq!(client_middleware.before_count.load(Ordering::SeqCst), 1);
    assert_eq!(client_middleware.response_count.load(Ordering::SeqCst), 1);
    assert_eq!(request_middleware.before_count.load(Ordering::SeqCst), 1);
    assert_eq!(request_middleware.response_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn request_options_max_retries_overrides_client_default() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": "boom"}
        })))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let options = RequestOptions {
        max_retries: Some(1),
        ..Default::default()
    };

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/retry", None, None, Some(&options))
        .await;
    assert!(result.is_err());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-retry-count")
            .and_then(|v| v.to_str().ok()),
        Some("0")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-stainless-retry-count")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );
}

#[tokio::test]
async fn request_options_can_remove_retry_count_header_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-no-header"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": "boom"}
        })))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 1);
    let mut headers = HashMap::new();
    headers.insert("X-Stainless-Retry-Count".to_owned(), None);
    let options = RequestOptions {
        headers: Some(headers),
        ..Default::default()
    };

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/retry-no-header", None, None, Some(&options))
        .await;
    assert!(result.is_err());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.headers.get("x-stainless-retry-count").is_none()));
}

#[tokio::test]
async fn request_options_can_overwrite_retry_count_header_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-overwrite-header"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": "boom"}
        })))
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 1);
    let mut headers = HashMap::new();
    headers.insert("X-Stainless-Retry-Count".to_owned(), Some("42".to_owned()));
    let options = RequestOptions {
        headers: Some(headers),
        ..Default::default()
    };

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/retry-overwrite-header", None, None, Some(&options))
        .await;
    assert!(result.is_err());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request
        .headers
        .get("x-stainless-retry-count")
        .and_then(|value| value.to_str().ok())
        == Some("42")));
}

#[tokio::test]
async fn default_headers_can_remove_retry_count_header_like_ts_omit_retry_count_header_by_default()
{
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-no-default-header"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "type": "error",
            "error": {"type": "api_error", "message": "boom"}
        })))
        .mount(&server)
        .await;

    let client = Anthropic::new(ClientOptions {
        api_key: Some("test-api-key".to_owned()),
        base_url: Some(server.uri()),
        max_retries: Some(1),
        default_headers: Some(HashMap::from([(
            "X-Stainless-Retry-Count".to_owned(),
            None,
        )])),
        ..Default::default()
    })
    .expect("client creation should succeed");

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/retry-no-default-header", None, None, None)
        .await;
    assert!(result.is_err());

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests
        .iter()
        .all(|request| request.headers.get("x-stainless-retry-count").is_none()));
}

#[tokio::test]
async fn raw_response_retries_on_429_with_retry_after_ms_like_ts_as_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/raw-retry-after-ms"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after-ms", "1")
                .set_body_json(serde_json::json!({
                    "type": "error",
                    "error": {"type": "rate_limit_error", "message": "slow down"}
                })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/raw-retry-after-ms"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({"a": 1})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 2);
    let response = client
        .get_raw_response("/v1/raw-retry-after-ms", None, None, None)
        .await
        .expect("raw response should retry and return final successful response");

    assert_eq!(response.status, 200);
    assert_eq!(response.text().unwrap(), r#"{"a":1}"#);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1]
            .headers
            .get("x-stainless-retry-count")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
}

#[tokio::test]
async fn retry_header_true_overrides_non_retryable_status_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-header-true"))
        .respond_with(
            ResponseTemplate::new(400)
                .insert_header("x-should-retry", "true")
                .insert_header("retry-after-ms", "1")
                .set_body_json(serde_json::json!({
                    "type": "error",
                    "error": {"type": "invalid_request_error", "message": "retry me"}
                })),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-header-true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 1);
    let value: serde_json::Value = client
        .get_with_options("/v1/retry-header-true", None, None, None)
        .await
        .unwrap();

    assert_eq!(value, serde_json::json!({"ok": true}));
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn retry_header_false_overrides_retryable_status_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/retry-header-false"))
        .respond_with(
            ResponseTemplate::new(500)
                .insert_header("x-should-retry", "false")
                .set_body_json(serde_json::json!({
                    "type": "error",
                    "error": {"type": "api_error", "message": "do not retry"}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 2);
    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/retry-header-false", None, None, None)
        .await;

    assert!(result.is_err());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
}

#[tokio::test]
async fn request_options_abort_signal_cancels_before_send() {
    let server = MockServer::start().await;
    let client = mock_client(&server.uri(), 0);

    let (handle, signal) = AbortSignal::pair();
    handle.abort();
    let options = RequestOptions {
        signal: Some(signal),
        ..Default::default()
    };

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/never-sent", None, None, Some(&options))
        .await;

    let err = result.unwrap_err();
    assert!(matches!(err, anthropic_sdk::ApiError::UserAbort { .. }));
    assert_eq!(err.to_string(), "Request was aborted.");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn request_options_abort_signal_cancels_in_flight_request_like_ts_custom_signal() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    let responder_attempts = Arc::clone(&attempts);

    Mock::given(method("GET"))
        .and(path("/v1/abort-in-flight"))
        .respond_with(move |_request: &wiremock::Request| {
            responder_attempts.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(500))
                .set_body_json(serde_json::json!({"too_late": true}))
        })
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri(), 0);
    let (handle, signal) = AbortSignal::pair();
    let abort_attempts = Arc::clone(&attempts);
    tokio::spawn(async move {
        for _ in 0..50 {
            if abort_attempts.load(Ordering::SeqCst) > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        handle.abort();
    });

    let options = RequestOptions {
        signal: Some(signal),
        ..Default::default()
    };

    let result: Result<serde_json::Value, _> = client
        .get_with_options("/v1/abort-in-flight", None, None, Some(&options))
        .await;

    let err = result.unwrap_err();
    assert!(matches!(err, anthropic_sdk::ApiError::UserAbort { .. }));
    assert_eq!(err.to_string(), "Request was aborted.");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

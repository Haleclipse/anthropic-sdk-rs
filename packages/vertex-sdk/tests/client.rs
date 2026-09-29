use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anthropic_sdk::resources::beta::messages::{
    BetaMessageContent, BetaMessageCountTokensParams, BetaMessageCreateParams, BetaMessageParam,
};
use anthropic_sdk::sdk_lib::tools::BetaToolRunnerParams;
use anthropic_sdk::{
    ClientOptions as CoreClientOptions, MessageContent, MessageCountTokensParams,
    MessageCreateParams, MessageParam, RequestOptions,
};
use anthropic_sdk_vertex::{
    create_client, create_client_with_core_options, rewrite_url, AnthropicVertex,
    ClientOptions as VertexClientOptions, TokenProvider, VertexConfig, ANTHROPIC_VERSION,
};
use serde_json::Value;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../../../tests/support/child_env.rs"]
mod child_env;

fn vertex_config(project_id: &str, region: &str) -> VertexConfig {
    VertexConfig {
        project_id: project_id.to_owned(),
        region: region.to_owned(),
        access_token: Some("vertex-token".to_owned()),
        token_provider: None,
        base_url: None,
    }
}

#[test]
fn vertex_client_options_alias_matches_ts_export_name() {
    let _: VertexClientOptions = vertex_config("my-project", "global");
}

#[test]
fn rewrite_url_non_streaming() {
    let cfg = vertex_config("my-project", "us-east5");
    let url = rewrite_url("/v1/messages", "claude-sonnet-4-20250514", &cfg, false);
    assert_eq!(
        url,
        "https://us-east5-aiplatform.googleapis.com/v1/projects/my-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"
    );
}

#[test]
fn rewrite_url_streaming() {
    let cfg = vertex_config("my-project", "europe-west1");
    let url = rewrite_url("/v1/messages", "claude-sonnet-4-20250514", &cfg, true);
    assert_eq!(
        url,
        "https://europe-west1-aiplatform.googleapis.com/v1/projects/my-project/locations/europe-west1/publishers/anthropic/models/claude-sonnet-4-20250514:streamRawPredict"
    );
}

#[test]
fn rewrite_url_global_region() {
    let cfg = vertex_config("my-project", "global");
    let url = rewrite_url("/v1/messages", "claude-sonnet-4-20250514", &cfg, false);
    assert!(url.starts_with("https://aiplatform.googleapis.com/"));
    assert!(url.contains("/locations/global/"));
}

#[test]
fn base_url_uses_region_or_explicit_override() {
    let client = create_client(&vertex_config("test-project", "asia-southeast1")).unwrap();
    assert_eq!(
        client.base_url(),
        "https://asia-southeast1-aiplatform.googleapis.com/v1"
    );

    let mut cfg = vertex_config("test-project", "global");
    cfg.base_url = Some("https://test.googleapis.com".to_owned());
    let client = create_client(&cfg).unwrap();
    assert_eq!(client.base_url(), "https://test.googleapis.com");

    cfg.base_url = Some(String::new());
    let client = create_client(&cfg).unwrap();
    assert_eq!(client.base_url(), "https://aiplatform.googleapis.com/v1");
}

#[test]
fn core_options_are_preserved_like_ts_provider_extends_core_client_options() {
    let mut default_headers = std::collections::HashMap::new();
    default_headers.insert("x-vertex-default".to_owned(), Some("yes".to_owned()));
    let mut default_query = std::collections::HashMap::new();
    default_query.insert("provider_query".to_owned(), Some("1".to_owned()));

    let mut cfg = vertex_config("test-project", "global");
    cfg.base_url = Some("https://vertex.local/v1".to_owned());
    let client = create_client_with_core_options(
        &cfg,
        CoreClientOptions {
            max_retries: Some(6),
            timeout: Some(23_456),
            default_headers: Some(default_headers),
            default_query: Some(default_query),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(client.base_url(), "https://vertex.local/v1");
    assert_eq!(client.max_retries(), 6);
    assert_eq!(client.timeout(), 23_456);
    assert_eq!(
        client.build_url("/foo", None).unwrap().as_str(),
        "https://vertex.local/v1/foo?provider_query=1"
    );
    let headers = client.build_headers(0, None).unwrap();
    assert_eq!(headers.get("x-vertex-default").unwrap(), "yes");
    assert!(headers.get("x-api-key").is_none());
    assert_eq!(headers.get("authorization").unwrap(), "Bearer vertex-token");
}

#[test]
fn anthropic_vertex_wrapper_derefs_to_core_client() {
    let cfg = vertex_config("my-project", "global");
    let client = AnthropicVertex::new(&cfg).unwrap();
    assert_eq!(client.base_url(), "https://aiplatform.googleapis.com/v1");
    assert_eq!(
        client.as_client().base_url(),
        "https://aiplatform.googleapis.com/v1"
    );
}

#[tokio::test]
async fn messages_create_rewrites_to_raw_predict_path_and_body_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let _ = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-20250514".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert_eq!(body["stream"], false);
    assert!(body.get("model").is_none());
}

#[tokio::test]
async fn messages_create_request_options_path_bypasses_vertex_model_rewrite_like_ts_build_request()
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/custom/vertex/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_custom_path",
            "type": "message",
            "role": "assistant",
            "model": "original-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let options = RequestOptions {
        path: Some("/custom/vertex/messages".to_owned()),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("original body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_custom_path");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(requests[0].url.path(), "/custom/vertex/messages");
    assert_eq!(body["model"], "original-model");
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn messages_create_custom_path_falsy_body_suppresses_body_like_ts_build_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/custom/vertex/no-body"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_no_body",
            "type": "message",
            "role": "assistant",
            "model": "original-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let options = RequestOptions {
        path: Some("/custom/vertex/no-body".to_owned()),
        body: Some(serde_json::Value::Bool(false)),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("generated body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_no_body");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url.path(), "/custom/vertex/no-body");
    assert!(
        requests[0].body.is_empty(),
        "body was {:?}",
        requests[0].body
    );
    assert!(requests[0].headers.get("content-type").is_none());
}

#[tokio::test]
async fn messages_create_applies_request_options_body_before_vertex_rewrite_like_ts_build_request()
{
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/override-model:rawPredict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_override",
            "type": "message",
            "role": "assistant",
            "model": "override-model",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let options = RequestOptions {
        body: Some(serde_json::json!({
            "model": "override-model",
            "max_tokens": 7,
            "messages": [{"role": "user", "content": "override body"}],
            "anthropic_version": "custom-version"
        })),
        ..Default::default()
    };

    let response = client
        .messages()
        .create_with_options(
            &MessageCreateParams {
                model: "original-model".to_owned(),
                max_tokens: 16,
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("original body".to_owned()),
                }],
                ..Default::default()
            },
            Some(&options),
        )
        .await
        .unwrap();
    assert_eq!(response.id, "msg_override");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(requests[0].url.path(), "/projects/my-project/locations/us-east5/publishers/anthropic/models/override-model:rawPredict");
    assert!(body.get("model").is_none());
    assert_eq!(body["stream"], false);
    assert_eq!(body["max_tokens"], 7);
    assert_eq!(body["messages"][0]["content"], "override body");
    assert_eq!(body["anthropic_version"], "custom-version");
}

fn beta_message_params(model: &str) -> BetaMessageCreateParams {
    BetaMessageCreateParams {
        model: model.to_owned(),
        max_tokens: 16,
        messages: vec![BetaMessageParam {
            role: "user".to_owned(),
            content: BetaMessageContent::Text("hello".to_owned()),
        }],
        betas: Some(vec!["beta-flag".to_owned()]),
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
        stream: Some(true),
        system: None,
        temperature: None,
        thinking: None,
        tool_choice: None,
        tools: None,
        top_k: None,
        top_p: None,
    }
}

#[tokio::test]
async fn beta_messages_create_rewrites_to_raw_predict_and_sends_beta_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let _ = client
        .beta()
        .messages()
        .create(&beta_message_params("claude-sonnet-4-20250514"))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
    assert_eq!(body["stream"], false);
    assert!(body.get("model").is_none());
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn provider_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/msg-model:rawPredict"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_vertex_message")
                .set_body_json(serde_json::json!({
                    "id": "msg_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "msg-model",
                    "content": [],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/beta-model:rawPredict"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_vertex_beta_message")
                .set_body_json(serde_json::json!({
                    "id": "msg_beta_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "beta-model",
                    "content": [],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/count-tokens:rawPredict"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_vertex_count")
                .set_body_json(serde_json::json!({"input_tokens": 42})),
        )
        .expect(2)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let message = client
        .messages()
        .create_with_response(&MessageCreateParams {
            model: "msg-model".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(message.data.id, "msg_test");
    assert_eq!(message.request_id.as_deref(), Some("req_vertex_message"));
    assert_eq!(
        message.response.header("request-id"),
        Some("req_vertex_message")
    );

    let count = client
        .messages()
        .countTokensWithResponse(&MessageCountTokensParams {
            model: "msg-model".to_owned(),
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            output_config: None,
            system: None,
            thinking: None,
            tool_choice: None,
            tools: None,
        })
        .await
        .unwrap();
    assert_eq!(count.data.input_tokens, 42);
    assert_eq!(count.request_id.as_deref(), Some("req_vertex_count"));

    let beta_message = client
        .beta()
        .messages()
        .create_with_response(&beta_message_params("beta-model"))
        .await
        .unwrap();
    assert_eq!(beta_message.data.id, "msg_beta_test");
    assert_eq!(
        beta_message.request_id.as_deref(),
        Some("req_vertex_beta_message")
    );

    let beta_count = client
        .beta()
        .messages()
        .countTokensWithResponse(&BetaMessageCountTokensParams {
            model: "beta-model".to_owned(),
            messages: vec![BetaMessageParam {
                role: "user".to_owned(),
                content: BetaMessageContent::Text("hello".to_owned()),
            }],
            betas: Some(vec!["beta-flag".to_owned()]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(beta_count.data.input_tokens, 42);
    assert_eq!(beta_count.request_id.as_deref(), Some("req_vertex_count"));

    let requests = server.received_requests().await.unwrap();
    let beta_count_request = requests
        .iter()
        .find(|request| {
            request.url.path().ends_with("/count-tokens:rawPredict")
                && request.headers.get("anthropic-beta").is_some()
        })
        .unwrap();
    assert_eq!(
        beta_count_request.headers.get("anthropic-beta").unwrap(),
        "beta-flag,token-counting-2024-11-01"
    );
}

#[tokio::test]
async fn count_tokens_request_overrides_follow_vertex_build_request_path_rules() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/custom/count-tokens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "input_tokens": 7
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/messages/count_tokens"))
        .and(query_param("beta", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "input_tokens": 8
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "global");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let stable = client
        .messages()
        .count_tokens_with_options(
            &MessageCountTokensParams {
                model: "stable-model".to_owned(),
                messages: vec![MessageParam {
                    role: "user".to_owned(),
                    content: MessageContent::Text("hello".to_owned()),
                }],
                output_config: None,
                system: None,
                thinking: None,
                tool_choice: None,
                tools: None,
            },
            Some(&RequestOptions {
                path: Some("/custom/count-tokens".to_owned()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(stable.input_tokens, 7);

    let beta = client
        .beta()
        .messages()
        .count_tokens_with_options(
            &BetaMessageCountTokensParams {
                model: "beta-model".to_owned(),
                messages: vec![BetaMessageParam {
                    role: "user".to_owned(),
                    content: BetaMessageContent::Text("hello".to_owned()),
                }],
                ..Default::default()
            },
            Some(&RequestOptions {
                method: Some(reqwest::Method::GET),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(beta.input_tokens, 8);

    let requests = server.received_requests().await.unwrap();
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["anthropic_version"], ANTHROPIC_VERSION);
        assert!(body.get("model").is_some());
    }
}

#[tokio::test]
async fn provider_stream_with_response_helpers_return_stream_metadata_and_request_id() {
    let server = MockServer::start().await;
    let sse = "event: ping\ndata: {}\n\n";
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/stream-model:streamRawPredict"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_vertex_stream")
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/beta-stream-model:streamRawPredict"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_vertex_beta_stream")
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let stream = client
        .messages()
        .create_stream_with_response(&MessageCreateParams {
            model: "stream-model".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(stream.request_id.as_deref(), Some("req_vertex_stream"));
    assert_eq!(
        stream.response.header("request-id"),
        Some("req_vertex_stream")
    );
    assert!(stream.response.body.is_empty());

    let beta_stream = client
        .beta()
        .messages()
        .stream_with_response(&beta_message_params("beta-stream-model"))
        .await
        .unwrap();
    assert_eq!(
        beta_stream.request_id.as_deref(),
        Some("req_vertex_beta_stream")
    );
    assert_eq!(
        beta_stream.response.header("request-id"),
        Some("req_vertex_beta_stream")
    );
    assert!(beta_stream.response.body.is_empty());

    let requests = server.received_requests().await.unwrap();
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["stream"], true);
    }
}

#[tokio::test]
async fn beta_messages_tool_runner_uses_raw_predict_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/my-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let mut create_params = beta_message_params("claude-sonnet-4-20250514");
    create_params.stream = None;
    let mut runner = client.beta().messages().toolRunner(BetaToolRunnerParams {
        create_params,
        tools: vec![],
        max_iterations: Some(1),
        compaction_control: None,
    });
    let _ = runner.run_until_done().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers.get("x-stainless-helper").unwrap(),
        "BetaToolRunner"
    );
}

#[tokio::test]
async fn access_token_sets_bearer_auth_and_disables_anthropic_api_key() {
    if child_env::run_in_child_env(
        module_path!(),
        "access_token_sets_bearer_auth_and_disables_anthropic_api_key",
        &[("ANTHROPIC_API_KEY", "ambient-key-should-not-leak")],
    ) {
        return;
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.base_url = Some(server.uri());
    let client = create_client(&cfg).unwrap();

    let _: Value = client.get("/v1/test", None, None).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer vertex-token"
    );
    assert!(requests[0].headers.get("x-api-key").is_none());
}

struct CountingTokenProvider {
    counter: Arc<AtomicUsize>,
}

impl TokenProvider for CountingTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, anthropic_sdk::ApiError>> {
        let counter = Arc::clone(&self.counter);
        Box::pin(async move {
            let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(format!("vertex-token-{next}"))
        })
    }
}

struct ProjectResolvingTokenProvider;

impl TokenProvider for ProjectResolvingTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, anthropic_sdk::ApiError>> {
        Box::pin(async { Ok("resolved-token".to_owned()) })
    }

    fn project_id(&self) -> Option<String> {
        Some("resolved-project".to_owned())
    }
}

#[tokio::test]
async fn token_provider_project_id_resolves_missing_project_like_ts_auth_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/projects/resolved-project/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-20250514:rawPredict"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = vertex_config("", "us-east5");
    cfg.access_token = None;
    cfg.token_provider = Some(Arc::new(ProjectResolvingTokenProvider));
    cfg.base_url = Some(server.uri());
    let client = AnthropicVertex::new(&cfg).unwrap();

    let _ = client
        .messages()
        .create(&MessageCreateParams {
            model: "claude-sonnet-4-20250514".to_owned(),
            max_tokens: 16,
            messages: vec![MessageParam {
                role: "user".to_owned(),
                content: MessageContent::Text("hello".to_owned()),
            }],
            ..Default::default()
        })
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer resolved-token"
    );
    assert!(requests[0].headers.get("x-api-key").is_none());
}

#[tokio::test]
async fn token_provider_is_invoked_per_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(2)
        .mount(&server)
        .await;

    let counter = Arc::new(AtomicUsize::new(0));
    let mut cfg = vertex_config("my-project", "us-east5");
    cfg.access_token = None;
    cfg.token_provider = Some(Arc::new(CountingTokenProvider {
        counter: Arc::clone(&counter),
    }));
    cfg.base_url = Some(server.uri());
    let client = create_client(&cfg).unwrap();

    let _: Value = client.get("/v1/test", None, None).await.unwrap();
    let _: Value = client.get("/v1/test", None, None).await.unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer vertex-token-1"
    );
    assert_eq!(
        requests[1].headers.get("authorization").unwrap(),
        "Bearer vertex-token-2"
    );
}

// `from_env` scenarios, one child environment each (`child_env`).

#[test]
fn from_env_requires_region() {
    if child_env::run_in_child_env(module_path!(), "from_env_requires_region", &[]) {
        return;
    }
    let result = VertexConfig::from_env();
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "SDK error: No region was given. The client should be instantiated with the `region` option or the `CLOUD_ML_REGION` environment variable should be set."
    );
}

#[test]
fn from_env_requires_project_id() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_requires_project_id",
        &[("CLOUD_ML_REGION", " primary-region ")],
    ) {
        return;
    }
    let result = VertexConfig::from_env();
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "SDK error: No projectId was given and it could not be resolved from credentials. The client should be instantiated with the `projectId` option or the `ANTHROPIC_VERTEX_PROJECT_ID` environment variable should be set."
    );
}

#[test]
fn from_env_reads_primary_names_and_trims() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_reads_primary_names_and_trims",
        &[
            ("CLOUD_ML_REGION", " primary-region "),
            ("ANTHROPIC_VERTEX_PROJECT_ID", " primary-project "),
            ("CLOUD_ML_PROJECT_ID", "ignored-project"),
            ("ANTHROPIC_VERTEX_REGION", "ignored-region"),
            (
                "ANTHROPIC_VERTEX_BASE_URL",
                " https://override.example.com ",
            ),
        ],
    ) {
        return;
    }
    let cfg = VertexConfig::from_env().unwrap();
    assert_eq!(cfg.project_id, "primary-project");
    assert_eq!(cfg.region, "primary-region");
    assert_eq!(
        cfg.base_url.as_deref(),
        Some("https://override.example.com")
    );
}

#[test]
fn create_client_validates_region_and_project_id_like_ts() {
    let mut cfg = vertex_config("test-project", "");
    let err = create_client(&cfg).unwrap_err();
    assert_eq!(
        err.to_string(),
        "SDK error: No region was given. The client should be instantiated with the `region` option or the `CLOUD_ML_REGION` environment variable should be set."
    );

    cfg.region = "us-east5".to_owned();
    cfg.project_id.clear();
    let err = create_client(&cfg).unwrap_err();
    assert_eq!(
        err.to_string(),
        "SDK error: No projectId was given and it could not be resolved from credentials. The client should be instantiated with the `projectId` option or the `ANTHROPIC_VERTEX_PROJECT_ID` environment variable should be set."
    );
}

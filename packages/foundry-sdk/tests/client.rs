use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anthropic_sdk::resources::beta::messages::{
    BetaMessageContent, BetaMessageCountTokensParams, BetaMessageCreateParams, BetaMessageParam,
};
use anthropic_sdk::sdk_lib::tools::BetaToolRunnerParams;
use anthropic_sdk::{
    ApiError, ClientOptions as CoreClientOptions, MessageContent, MessageCountTokensParams,
    MessageCreateParams, MessageParam,
};
use anthropic_sdk_foundry::{
    AnthropicFoundry, FoundryClientOptions, FoundryConfig, TokenProvider, TokenProviderError,
    base_url, create_client, create_client_with_core_options,
};
use serde_json::Value;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[path = "../../../tests/support/child_env.rs"]
mod child_env;

fn foundry_config(resource: &str) -> FoundryConfig {
    FoundryConfig {
        resource: resource.to_owned(),
        api_key: Some("test-key".to_owned()),
        token_provider: None,
        base_url: None,
    }
}

#[test]
fn foundry_client_options_alias_matches_ts_export_name() {
    let _: FoundryClientOptions = foundry_config("example-resource");
}

#[test]
fn base_url_format() {
    assert_eq!(
        base_url("my-resource"),
        "https://my-resource.services.ai.azure.com/anthropic/"
    );
}

#[test]
fn base_url_with_hyphens() {
    assert_eq!(
        base_url("west-us-2-prod"),
        "https://west-us-2-prod.services.ai.azure.com/anthropic/"
    );
}

#[test]
fn config_debug_redacts_key() {
    let cfg = foundry_config("test");
    let debug_str = format!("{cfg:?}");
    assert!(!debug_str.contains("test-key"));
    assert!(debug_str.contains("***"));
}

#[test]
fn create_client_with_api_key_and_base_url_only() {
    let client = create_client(foundry_config("example-resource")).unwrap();
    assert_eq!(
        client.base_url(),
        "https://example-resource.services.ai.azure.com/anthropic/"
    );

    let mut cfg = foundry_config("");
    cfg.base_url = Some("http://localhost:4010".to_owned());
    let client = create_client(cfg).unwrap();
    assert_eq!(client.base_url(), "http://localhost:4010");
}

#[test]
fn core_options_are_preserved_like_ts_provider_extends_core_client_options() {
    let mut default_headers = std::collections::HashMap::new();
    default_headers.insert("x-foundry-default".to_owned(), Some("yes".to_owned()));
    let mut default_query = std::collections::HashMap::new();
    default_query.insert("provider_query".to_owned(), Some("1".to_owned()));

    let mut cfg = foundry_config("");
    cfg.base_url = Some("https://foundry.local/anthropic".to_owned());
    let client = create_client_with_core_options(
        cfg,
        CoreClientOptions {
            max_retries: Some(7),
            timeout: Some(34_567),
            default_headers: Some(default_headers),
            default_query: Some(default_query),
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(client.base_url(), "https://foundry.local/anthropic");
    assert_eq!(client.max_retries(), 7);
    assert_eq!(client.timeout(), 34_567);
    assert_eq!(
        client.build_url("/v1/test", None).unwrap().as_str(),
        "https://foundry.local/anthropic/v1/test?provider_query=1"
    );
    let headers = client.build_headers(0, None).unwrap();
    assert_eq!(headers.get("x-foundry-default").unwrap(), "yes");
    assert_eq!(headers.get("x-api-key").unwrap(), "test-key");
}

/// TS `AnthropicFoundry` overrides `authHeaders` (`client.ts:103-131`): only
/// the Foundry key is sent, never `ANTHROPIC_API_KEY` or
/// `ANTHROPIC_AUTH_TOKEN` from the environment.
#[test]
fn api_key_mode_never_sends_anthropic_env_credentials() {
    if child_env::run_in_child_env(
        module_path!(),
        "api_key_mode_never_sends_anthropic_env_credentials",
        &[
            ("ANTHROPIC_API_KEY", "ambient-key"),
            ("ANTHROPIC_AUTH_TOKEN", "ambient-token"),
        ],
    ) {
        return;
    }
    let client = create_client(foundry_config("example-resource")).unwrap();
    let headers = client.build_headers(0, None).unwrap();
    assert_eq!(headers.get("x-api-key").unwrap(), "test-key");
    assert!(headers.get("authorization").is_none());
}

#[test]
fn create_client_rejects_base_url_and_resource_like_ts() {
    let mut cfg = foundry_config("example-resource");
    cfg.base_url = Some("http://localhost:4010".to_owned());

    let result = create_client(cfg);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "SDK error: baseURL and resource are mutually exclusive"
    );
}

#[test]
fn anthropic_foundry_wrapper_derefs_to_core_client() {
    let client = AnthropicFoundry::new(foundry_config("example-resource")).unwrap();
    assert_eq!(
        client.base_url(),
        "https://example-resource.services.ai.azure.com/anthropic/"
    );
    assert_eq!(
        client.as_client().base_url(),
        "https://example-resource.services.ai.azure.com/anthropic/"
    );
}

#[tokio::test]
async fn foundry_messages_wrapper_uses_core_message_endpoint_without_batches_surface() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
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

    let mut cfg = foundry_config("");
    cfg.base_url = Some(server.uri());
    let client = AnthropicFoundry::new(cfg).unwrap();

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
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], false);
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

#[tokio::test]
async fn foundry_beta_messages_wrapper_preserves_beta_header_and_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
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

    let mut cfg = foundry_config("");
    cfg.base_url = Some(server.uri());
    let client = AnthropicFoundry::new(cfg).unwrap();

    let _ = client
        .beta()
        .messages()
        .create(&beta_message_params("claude-sonnet-4-20250514"))
        .await
        .unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].headers.get("anthropic-beta").unwrap(),
        "beta-flag"
    );
}

#[tokio::test]
async fn provider_with_response_helpers_return_data_raw_response_and_request_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_foundry_message")
                .set_body_json(serde_json::json!({
                    "id": "msg_test",
                    "type": "message",
                    "role": "assistant",
                    "model": "claude-sonnet-4-20250514",
                    "content": [],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {"input_tokens": 1, "output_tokens": 1}
                })),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages/count_tokens"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_foundry_count")
                .set_body_json(serde_json::json!({"input_tokens": 42})),
        )
        .expect(2)
        .mount(&server)
        .await;

    let mut cfg = foundry_config("");
    cfg.base_url = Some(server.uri());
    let client = AnthropicFoundry::new(cfg).unwrap();

    let message_params = MessageCreateParams {
        model: "claude-sonnet-4-20250514".to_owned(),
        max_tokens: 16,
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("hello".to_owned()),
        }],
        ..Default::default()
    };
    let message = client
        .messages()
        .create_with_response(&message_params)
        .await
        .unwrap();
    assert_eq!(message.data.id, "msg_test");
    assert_eq!(message.request_id.as_deref(), Some("req_foundry_message"));
    assert_eq!(
        message.response.header("request-id"),
        Some("req_foundry_message")
    );

    let count_params = MessageCountTokensParams {
        model: "claude-sonnet-4-20250514".to_owned(),
        messages: vec![MessageParam {
            role: "user".to_owned(),
            content: MessageContent::Text("hello".to_owned()),
        }],
        output_config: None,
        system: None,
        thinking: None,
        tool_choice: None,
        tools: None,
    };
    let count = client
        .messages()
        .countTokensWithResponse(&count_params)
        .await
        .unwrap();
    assert_eq!(count.data.input_tokens, 42);
    assert_eq!(count.request_id.as_deref(), Some("req_foundry_count"));

    let beta_message = client
        .beta()
        .messages()
        .create_with_response(&beta_message_params("claude-sonnet-4-20250514"))
        .await
        .unwrap();
    assert_eq!(beta_message.data.id, "msg_test");
    assert_eq!(
        beta_message.request_id.as_deref(),
        Some("req_foundry_message")
    );

    let beta_count = client
        .beta()
        .messages()
        .countTokensWithResponse(&BetaMessageCountTokensParams {
            model: "claude-sonnet-4-20250514".to_owned(),
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
    assert_eq!(beta_count.request_id.as_deref(), Some("req_foundry_count"));

    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().any(|request| {
        request.url.path() == "/v1/messages/count_tokens"
            && request.url.query() == Some("beta=true")
            && request.headers.get("anthropic-beta").unwrap()
                == "beta-flag,token-counting-2024-11-01"
    }));
}

#[tokio::test]
async fn provider_stream_with_response_helpers_return_stream_metadata_and_request_id() {
    let server = MockServer::start().await;
    let sse = "event: ping\ndata: {}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(query_param_is_missing("beta"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_foundry_stream")
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(query_param("beta", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("request-id", "req_foundry_beta_stream")
                .set_body_string(sse),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut cfg = foundry_config("");
    cfg.base_url = Some(server.uri());
    let client = AnthropicFoundry::new(cfg).unwrap();

    let stream = client
        .messages()
        .create_stream_with_response(&MessageCreateParams {
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
    assert_eq!(stream.request_id.as_deref(), Some("req_foundry_stream"));
    assert_eq!(
        stream.response.header("request-id"),
        Some("req_foundry_stream")
    );
    assert!(stream.response.body.is_empty());

    let beta_stream = client
        .beta()
        .messages()
        .stream_with_response(&beta_message_params("claude-sonnet-4-20250514"))
        .await
        .unwrap();
    assert_eq!(
        beta_stream.request_id.as_deref(),
        Some("req_foundry_beta_stream")
    );
    assert_eq!(
        beta_stream.response.header("request-id"),
        Some("req_foundry_beta_stream")
    );
    assert!(beta_stream.response.body.is_empty());

    let requests = server.received_requests().await.unwrap();
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["stream"], true);
    }
}

#[tokio::test]
async fn foundry_beta_messages_tool_runner_uses_foundry_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
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

    let mut cfg = foundry_config("");
    cfg.base_url = Some(server.uri());
    let client = AnthropicFoundry::new(cfg).unwrap();

    let mut runner = client.beta().messages().toolRunner(BetaToolRunnerParams {
        create_params: beta_message_params("claude-sonnet-4-20250514"),
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

struct DummyTokenProvider;

impl TokenProvider for DummyTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, TokenProviderError>> {
        Box::pin(async { Ok("test-token".to_owned()) })
    }
}

/// TS `apiKey: azureADTokenProvider ?? apiKey` plus the `authHeaders`
/// override: in token mode only the Bearer token is sent, even when an empty
/// key (falsy, so accepted beside the provider) was passed too.
#[tokio::test]
async fn token_mode_sends_no_x_api_key_even_with_an_empty_key_like_ts() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let client = create_client(FoundryConfig {
        resource: String::new(),
        api_key: Some(String::new()),
        token_provider: Some(Box::new(DummyTokenProvider)),
        base_url: Some(server.uri()),
    })
    .unwrap();
    let _: Value = client.get("/v1/test", None, None).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("x-api-key").is_none());
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer test-token"
    );
}

#[test]
fn create_client_with_token_provider() {
    let config = FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: None,
        token_provider: Some(Box::new(DummyTokenProvider)),
        base_url: None,
    };
    let client = create_client(config).unwrap();
    assert_eq!(
        client.base_url(),
        "https://example-resource.services.ai.azure.com/anthropic/"
    );
}

struct EmptyTokenProvider;

impl TokenProvider for EmptyTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, TokenProviderError>> {
        Box::pin(async { Ok(String::new()) })
    }
}

struct FailingTokenProvider;

impl TokenProvider for FailingTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, TokenProviderError>> {
        Box::pin(async { Err(ApiError::Sdk("provider sentinel".to_owned()).into()) })
    }
}

/// A provider failing with something other than an `ApiError`, as a
/// credential library does.
struct ForeignErrorTokenProvider;

impl TokenProvider for ForeignErrorTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, TokenProviderError>> {
        Box::pin(async { Err("ChainedTokenCredential authentication failed.".into()) })
    }
}

struct CountingTokenProvider {
    counter: Arc<AtomicUsize>,
}

impl TokenProvider for CountingTokenProvider {
    fn get_token(&self) -> futures::future::BoxFuture<'_, Result<String, TokenProviderError>> {
        let counter = Arc::clone(&self.counter);
        Box::pin(async move {
            let next = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(format!("token-{next}"))
        })
    }
}

#[tokio::test]
async fn token_provider_is_invoked_per_request_and_sets_bearer_auth() {
    if child_env::run_in_child_env(
        module_path!(),
        "token_provider_is_invoked_per_request_and_sets_bearer_auth",
        &[("ANTHROPIC_API_KEY", "ambient-key-should-not-leak")],
    ) {
        return;
    }

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .expect(2)
        .mount(&server)
        .await;

    let counter = Arc::new(AtomicUsize::new(0));
    let config = FoundryConfig {
        resource: String::new(),
        api_key: None,
        token_provider: Some(Box::new(CountingTokenProvider {
            counter: Arc::clone(&counter),
        })),
        base_url: Some(server.uri()),
    };
    let client = create_client(config)
        .unwrap()
        .with_options(CoreClientOptions {
            max_retries: Some(0),
            ..Default::default()
        })
        .unwrap();

    let _: Value = client.get("/v1/test", None, None).await.unwrap();
    let _: Value = client.get("/v1/test", None, None).await.unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 2);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].headers.get("authorization").unwrap(),
        "Bearer token-1"
    );
    assert_eq!(
        requests[1].headers.get("authorization").unwrap(),
        "Bearer token-2"
    );
    assert!(requests[0].headers.get("x-api-key").is_none());
    assert!(requests[1].headers.get("x-api-key").is_none());
}

#[test]
fn foundry_constructor_truthiness_matches_ts_for_empty_values() {
    let client = create_client(FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: Some("test-key".to_owned()),
        token_provider: None,
        base_url: Some(String::new()),
    })
    .unwrap();
    assert_eq!(
        client.base_url(),
        "https://example-resource.services.ai.azure.com/anthropic/"
    );

    let client = create_client(FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: Some(String::new()),
        token_provider: Some(Box::new(DummyTokenProvider)),
        base_url: None,
    });
    assert!(
        client.is_ok(),
        "empty apiKey is falsey in the TS constructor"
    );

    let error = create_client(FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: Some("test-key".to_owned()),
        token_provider: None,
        base_url: Some(" ".to_owned()),
    })
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "SDK error: baseURL and resource are mutually exclusive"
    );
}

#[test]
fn create_client_rejects_both_auth_methods() {
    let config = FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: Some("test-key".to_owned()),
        token_provider: Some(Box::new(DummyTokenProvider)),
        base_url: None,
    };
    let result = create_client(config);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert_eq!(
        err_msg,
        "SDK error: The `apiKey` and `azureADTokenProvider` arguments are mutually exclusive; only one can be passed at a time."
    );
}

#[test]
fn create_client_rejects_no_auth() {
    let config = FoundryConfig {
        resource: "example-resource".to_owned(),
        api_key: None,
        token_provider: None,
        base_url: None,
    };
    let result = create_client(config);
    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert_eq!(
        err_msg,
        "SDK error: Missing credentials. Please pass one of `apiKey` and `azureTokenProvider`, or set the `ANTHROPIC_FOUNDRY_API_KEY` environment variable."
    );
}

#[tokio::test]
async fn token_provider_api_errors_are_rethrown_unchanged_like_ts() {
    let server = MockServer::start().await;
    let client = create_client(FoundryConfig {
        resource: String::new(),
        api_key: None,
        token_provider: Some(Box::new(FailingTokenProvider)),
        base_url: Some(server.uri()),
    })
    .unwrap();

    let error = client
        .get::<Value>("/v1/test", None, None)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "SDK error: provider sentinel");
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// TS `authHeaders` (`client.ts:105-115`) wraps a non-`AnthropicError`.
#[tokio::test]
async fn token_provider_foreign_errors_are_wrapped_like_ts() {
    let server = MockServer::start().await;
    let client = create_client(FoundryConfig {
        resource: String::new(),
        api_key: None,
        token_provider: Some(Box::new(ForeignErrorTokenProvider)),
        base_url: Some(server.uri()),
    })
    .unwrap();

    let error = client
        .get::<Value>("/v1/test", None, None)
        .await
        .unwrap_err();
    assert!(matches!(&error, ApiError::Sdk(message) if message
        == "Failed to get token from azureADTokenProvider: ChainedTokenCredential authentication failed."));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn token_provider_empty_string_is_rejected_like_ts() {
    let server = MockServer::start().await;
    let config = FoundryConfig {
        resource: String::new(),
        api_key: None,
        token_provider: Some(Box::new(EmptyTokenProvider)),
        base_url: Some(server.uri()),
    };
    let client = create_client(config).unwrap();
    let err = client
        .get::<Value>("/v1/test", None, None)
        .await
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "SDK error: Expected azureADTokenProvider function argument to return a string but it returned "
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

// `from_env` scenarios, one child environment each (`child_env`).

#[test]
fn from_env_requires_resource_or_base_url() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_requires_resource_or_base_url",
        &[],
    ) {
        return;
    }
    assert!(FoundryConfig::from_env().is_err());
}

#[test]
fn from_env_reads_primary_resource_and_key_and_trims() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_reads_primary_resource_and_key_and_trims",
        &[
            ("ANTHROPIC_FOUNDRY_RESOURCE", " primary-res "),
            ("AZURE_AI_FOUNDRY_RESOURCE", "ignored-fallback-res"),
            ("ANTHROPIC_FOUNDRY_API_KEY", " primary-key "),
            ("AZURE_AI_FOUNDRY_API_KEY", "ignored-fallback-key"),
        ],
    ) {
        return;
    }
    let cfg = FoundryConfig::from_env().unwrap();
    assert_eq!(cfg.resource, "primary-res");
    assert_eq!(cfg.api_key.as_deref(), Some("primary-key"));
    assert!(cfg.base_url.is_none());
}

#[test]
fn from_env_reads_base_url_without_resource() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_reads_base_url_without_resource",
        &[
            ("ANTHROPIC_FOUNDRY_API_KEY", " primary-key "),
            ("AZURE_AI_FOUNDRY_API_KEY", "ignored-fallback-key"),
            (
                "ANTHROPIC_FOUNDRY_BASE_URL",
                " https://override.example.com ",
            ),
        ],
    ) {
        return;
    }
    let cfg = FoundryConfig::from_env().unwrap();
    assert_eq!(cfg.resource, "");
    assert_eq!(cfg.api_key.as_deref(), Some("primary-key"));
    assert_eq!(
        cfg.base_url.as_deref(),
        Some("https://override.example.com")
    );
}

#[test]
fn from_env_rejects_resource_with_base_url() {
    if child_env::run_in_child_env(
        module_path!(),
        "from_env_rejects_resource_with_base_url",
        &[
            ("ANTHROPIC_FOUNDRY_RESOURCE", "primary-res"),
            ("ANTHROPIC_FOUNDRY_API_KEY", " primary-key "),
            (
                "ANTHROPIC_FOUNDRY_BASE_URL",
                " https://override.example.com ",
            ),
        ],
    ) {
        return;
    }
    let result = FoundryConfig::from_env();
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "SDK error: baseURL and resource are mutually exclusive"
    );
}

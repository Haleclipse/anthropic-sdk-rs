// Integration/parity tests for beta ToolRunner.
//
// Mirrors TS SDK lib/tools/BetaToolRunner behavior for the non-streaming
// automatic tool loop.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anthropic_sdk::resources::beta::messages::{
    BetaContentBlockParam, BetaMessageContent, BetaMessageCreateParams, BetaMessageParam,
    BetaTextBlockParam, BetaToolResultContent, BetaToolResultContentBlockParam,
    BetaToolUseBlockParam,
};
use anthropic_sdk::sdk_lib::tools::{
    BetaToolRunnerParams, CompactionControl, RunnableTool, ToolError,
};
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

fn beta_create_params() -> BetaMessageCreateParams {
    BetaMessageCreateParams {
        model: "claude-3-5-sonnet-latest".to_owned(),
        max_tokens: 128,
        messages: vec![BetaMessageParam {
            role: "user".to_owned(),
            content: BetaMessageContent::Text("What is the weather?".to_owned()),
        }],
        ..Default::default()
    }
}

struct WeatherTool;
struct HelperMarkedTool;
struct BlocksTool;
struct ErrorBlocksTool;
struct CountingTool {
    runs: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl RunnableTool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "get_weather",
            "description": "Get weather for a location",
            "input_schema": {
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                },
                "required": ["location"]
            }
        })
    }

    fn parse(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        if input.get("location").is_some() {
            return Ok(input);
        }
        if let Some(city) = input.get("city").and_then(|value| value.as_str()) {
            return Ok(serde_json::json!({"location": city}));
        }
        Err(ToolError::new("missing location"))
    }

    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
        Ok(format!(
            "Sunny in {}",
            input["location"].as_str().unwrap_or("unknown")
        ))
    }
}

#[async_trait::async_trait]
impl RunnableTool for HelperMarkedTool {
    fn name(&self) -> &str {
        "helper_tool"
    }

    fn stainless_helper(&self) -> Option<&str> {
        Some("mcpTool")
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "helper_tool",
            "description": "Helper-created tool",
            "input_schema": {"type": "object"}
        })
    }

    async fn run(&self, _input: serde_json::Value) -> Result<String, ToolError> {
        Ok("ok".to_owned())
    }
}

#[async_trait::async_trait]
impl RunnableTool for BlocksTool {
    fn name(&self) -> &str {
        "blocks_tool"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "blocks_tool",
            "description": "Return structured beta tool-result content blocks",
            "input_schema": {"type": "object"}
        })
    }

    async fn run_beta_tool_result_content(
        &self,
        _input: serde_json::Value,
    ) -> Result<BetaToolResultContent, ToolError> {
        Ok(BetaToolResultContent::Blocks(vec![
            BetaToolResultContentBlockParam::Text(BetaTextBlockParam {
                text: "structured block output".to_owned(),
                stainless_helpers: Vec::new(),
                cache_control: None,
                citations: None,
            }),
        ]))
    }

    async fn run(&self, _input: serde_json::Value) -> Result<String, ToolError> {
        Ok("fallback text output".to_owned())
    }
}

#[async_trait::async_trait]
impl RunnableTool for ErrorBlocksTool {
    fn name(&self) -> &str {
        "error_blocks_tool"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "error_blocks_tool",
            "description": "Return structured beta error content blocks",
            "input_schema": {"type": "object"}
        })
    }

    async fn run(&self, _input: serde_json::Value) -> Result<String, ToolError> {
        Err(ToolError::with_content_blocks(vec![
            BetaToolResultContentBlockParam::Text(BetaTextBlockParam {
                text: "structured error output".to_owned(),
                stainless_helpers: Vec::new(),
                cache_control: None,
                citations: None,
            }),
        ]))
    }
}

#[async_trait::async_trait]
impl RunnableTool for CountingTool {
    fn name(&self) -> &str {
        "counting_tool"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "counting_tool",
            "description": "Count executions",
            "input_schema": {"type": "object"}
        })
    }

    async fn run(&self, _input: serde_json::Value) -> Result<String, ToolError> {
        let next = self.runs.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("run {next}"))
    }
}

#[tokio::test]
async fn beta_tool_runner_generate_tool_response_and_message_mutators_match_ts_surface() {
    let client = mock_client("http://localhost:1");
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(WeatherTool)],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);

    runner.pushMessages(vec![BetaMessageParam {
        role: "user".to_owned(),
        content: BetaMessageContent::Text("also Boston".to_owned()),
    }]);
    assert_eq!(runner.params().create_params.messages.len(), 2);

    let assistant = BetaMessageParam {
        role: "assistant".to_owned(),
        content: BetaMessageContent::Blocks(vec![BetaContentBlockParam::ToolUse(
            BetaToolUseBlockParam {
                id: "toolu_1".to_owned(),
                input: serde_json::json!({"city": "Boston"}),
                name: "get_weather".to_owned(),
                cache_control: None,
                caller: None,
            },
        )]),
    };

    let tool_message = runner
        .generateToolResponse(&assistant)
        .await
        .unwrap()
        .expect("tool response should be generated");
    assert_eq!(tool_message.role, "user");
    match tool_message.content {
        BetaMessageContent::Blocks(blocks) => match &blocks[0] {
            BetaContentBlockParam::ToolResult(result) => {
                assert_eq!(result.tool_use_id, "toolu_1");
                assert_eq!(
                    serde_json::to_value(&result.content).unwrap(),
                    serde_json::json!("Sunny in Boston")
                );
            }
            other => panic!("expected tool result, got {other:?}"),
        },
        other => panic!("expected block content, got {other:?}"),
    }

    let mut replacement = beta_create_params();
    replacement.model = "claude-opus-4-6".to_owned();
    runner.setMessagesParams(replacement);
    assert_eq!(runner.params().create_params.model, "claude-opus-4-6");

    runner.setMessagesParamsWith(|previous| {
        let mut next = previous.clone();
        next.max_tokens = 77;
        next
    });
    assert_eq!(runner.params().create_params.max_tokens, 77);
}

#[tokio::test]
async fn beta_tool_runner_generate_tool_response_caches_until_params_mutate_like_ts() {
    let client = mock_client("http://localhost:1");
    let runs = Arc::new(AtomicUsize::new(0));
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(CountingTool {
            runs: Arc::clone(&runs),
        })],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);

    let assistant = BetaMessageParam {
        role: "assistant".to_owned(),
        content: BetaMessageContent::Blocks(vec![BetaContentBlockParam::ToolUse(
            BetaToolUseBlockParam {
                id: "toolu_count".to_owned(),
                input: serde_json::json!({}),
                name: "counting_tool".to_owned(),
                cache_control: None,
                caller: None,
            },
        )]),
    };

    let first = runner
        .generateToolResponse(&assistant)
        .await
        .unwrap()
        .expect("tool response");
    let second = runner
        .generate_tool_response(&assistant)
        .await
        .unwrap()
        .expect("cached tool response");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );

    runner.setMessagesParamsWith(|previous| previous.clone());
    let third = runner
        .generate_tool_response(&assistant)
        .await
        .unwrap()
        .expect("tool response after mutation");
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_ne!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&third).unwrap()
    );
}

#[tokio::test]
async fn beta_tool_runner_done_alias_caches_final_message_like_ts_completion_promise() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Done."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);

    let first = runner.done().await.unwrap();
    let second = runner.runUntilDone().await.unwrap();

    assert_eq!(first.id, "msg_final");
    assert_eq!(second.id, "msg_final");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn beta_tool_runner_is_awaitable_like_ts_thenable_runner() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_awaitable",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Awaited."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);

    let final_message = (&mut runner).await.unwrap();
    assert_eq!(final_message.id, "msg_awaitable");

    // The borrowing await path preserves the runner and shares the same cached
    // completion used by `done()` / `runUntilDone()`.
    assert_eq!(runner.done().await.unwrap().id, "msg_awaitable");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn beta_tool_runner_supports_structured_tool_result_content_blocks() {
    let client = mock_client("http://localhost:1");
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(BlocksTool)],
        max_iterations: None,
        compaction_control: None,
    };
    let runner = client.beta().messages().tool_runner(params);

    let assistant = BetaMessageParam {
        role: "assistant".to_owned(),
        content: BetaMessageContent::Blocks(vec![BetaContentBlockParam::ToolUse(
            BetaToolUseBlockParam {
                id: "toolu_blocks".to_owned(),
                input: serde_json::json!({}),
                name: "blocks_tool".to_owned(),
                cache_control: None,
                caller: None,
            },
        )]),
    };

    let tool_message = runner
        .generate_tool_response(&assistant)
        .await
        .unwrap()
        .expect("tool response should be generated");

    let BetaMessageContent::Blocks(blocks) = tool_message.content else {
        panic!("expected block content");
    };
    let BetaContentBlockParam::ToolResult(result) = &blocks[0] else {
        panic!("expected tool_result block");
    };
    assert_eq!(
        serde_json::to_value(&result.content).unwrap(),
        serde_json::json!([{
            "type": "text",
            "text": "structured block output"
        }])
    );
}

#[tokio::test]
async fn beta_tool_runner_preserves_structured_tool_error_content_blocks_like_ts_tool_error() {
    let client = mock_client("http://localhost:1");
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(ErrorBlocksTool)],
        max_iterations: None,
        compaction_control: None,
    };
    let runner = client.beta().messages().tool_runner(params);

    let assistant = BetaMessageParam {
        role: "assistant".to_owned(),
        content: BetaMessageContent::Blocks(vec![BetaContentBlockParam::ToolUse(
            BetaToolUseBlockParam {
                id: "toolu_error_blocks".to_owned(),
                input: serde_json::json!({}),
                name: "error_blocks_tool".to_owned(),
                cache_control: None,
                caller: None,
            },
        )]),
    };

    let tool_message = runner
        .generate_tool_response(&assistant)
        .await
        .unwrap()
        .expect("tool response should be generated");

    let BetaMessageContent::Blocks(blocks) = tool_message.content else {
        panic!("expected block content");
    };
    let BetaContentBlockParam::ToolResult(result) = &blocks[0] else {
        panic!("expected tool_result block");
    };
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        serde_json::to_value(&result.content).unwrap(),
        serde_json::json!([{
            "type": "text",
            "text": "structured error output"
        }])
    );
}

#[tokio::test]
async fn beta_tool_runner_executes_tools_through_beta_messages_resource() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_tool_use",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"location": "NYC"}
            }],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 7}
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "NYC is sunny."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 8, "output_tokens": 4}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(WeatherTool)],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.runUntilDone().await.unwrap();

    assert_eq!(final_message.id, "msg_final");
    assert_eq!(runner.params().create_params.messages.len(), 4);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        let query: std::collections::HashMap<_, _> = request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(query.get("beta").map(String::as_str), Some("true"));
        assert_eq!(
            request.headers.get("x-stainless-helper").unwrap(),
            "BetaToolRunner"
        );
    }

    let first_body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(first_body["tools"][0]["name"], "get_weather");
    assert_eq!(first_body["stream"], false);

    let second_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    let tool_result = &second_body["messages"][2]["content"][0];
    assert_eq!(tool_result["type"], "tool_result");
    assert_eq!(tool_result["tool_use_id"], "toolu_1");
    assert_eq!(tool_result["content"], "Sunny in NYC");
}

#[tokio::test]
async fn beta_tool_runner_includes_helper_marked_tools_in_stainless_header() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Done."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 8, "output_tokens": 4}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(HelperMarkedTool)],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.run_until_done().await.unwrap();
    assert_eq!(final_message.id, "msg_final");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("BetaToolRunner, mcpTool")
    );

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["tools"][0]["name"], "helper_tool");
}

#[tokio::test]
async fn beta_tool_runner_includes_helper_marked_initial_messages_in_stainless_header_like_ts() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Done."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 8, "output_tokens": 4}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let mut create_params = beta_create_params();
    create_params.messages = vec![BetaMessageParam {
        role: "user".to_owned(),
        content: BetaMessageContent::Blocks(vec![BetaContentBlockParam::Text(
            BetaTextBlockParam {
                text: "helper-created content".to_owned(),
                stainless_helpers: vec!["mcpMessage".to_owned()],
                cache_control: None,
                citations: None,
            },
        )]),
    }];
    let params = BetaToolRunnerParams {
        create_params,
        tools: vec![],
        max_iterations: None,
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.run_until_done().await.unwrap();
    assert_eq!(final_message.id, "msg_final");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("BetaToolRunner, mcpMessage")
    );

    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["messages"][0]["content"][0]["text"],
        "helper-created content"
    );
    assert!(body["messages"][0]["content"][0]
        .get("stainless_helpers")
        .is_none());
}

#[tokio::test]
async fn beta_tool_runner_with_options_merges_request_headers() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Done."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![],
        max_iterations: None,
        compaction_control: None,
    };
    let mut headers = std::collections::HashMap::new();
    headers.insert("x-runner-option".to_owned(), Some("enabled".to_owned()));
    let mut runner = client.beta().messages().tool_runner_with_options(
        params,
        RequestOptions {
            headers: Some(headers),
            ..Default::default()
        },
    );

    let final_message = runner.run_until_done().await.unwrap();
    assert_eq!(final_message.id, "msg_final");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("BetaToolRunner")
    );
    assert_eq!(
        requests[0]
            .headers
            .get("x-runner-option")
            .and_then(|v| v.to_str().ok()),
        Some("enabled")
    );
}

#[tokio::test]
async fn beta_tool_runner_respects_max_iterations_without_extra_final_call() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_tool_use",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"location": "NYC"}
            }],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 7}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(WeatherTool)],
        max_iterations: Some(1),
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.run_until_done().await.unwrap();

    assert_eq!(final_message.id, "msg_tool_use");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(runner.params().create_params.messages.len(), 3);
}

#[tokio::test]
async fn beta_tool_runner_treats_zero_max_iterations_as_unbounded_like_ts_truthy_guard() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_tool_use",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"location": "NYC"}
            }],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 7}
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "done"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 6, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![Box::new(WeatherTool)],
        max_iterations: Some(0),
        compaction_control: None,
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.run_until_done().await.unwrap();

    assert_eq!(final_message.id, "msg_final");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
}

#[tokio::test]
async fn beta_tool_runner_compacts_when_token_threshold_is_exceeded() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_high_usage",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "A long response."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 11, "output_tokens": 1}
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_compaction",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-haiku-latest",
            "content": [{"type": "text", "text": "<summary>compressed</summary>"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 2, "output_tokens": 2}
        })))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_final_after_compaction",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-5-sonnet-latest",
            "content": [{"type": "text", "text": "Done after compaction."}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let client = mock_client(&server.uri());
    let params = BetaToolRunnerParams {
        create_params: beta_create_params(),
        tools: vec![],
        max_iterations: Some(3),
        compaction_control: Some(CompactionControl {
            enabled: true,
            context_token_threshold: Some(10),
            model: Some("claude-3-5-haiku-latest".to_owned()),
            summary_prompt: Some("Summarize the conversation.".to_owned()),
        }),
    };
    let mut runner = client.beta().messages().tool_runner(params);
    let final_message = runner.run_until_done().await.unwrap();
    assert_eq!(final_message.id, "msg_final_after_compaction");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("BetaToolRunner")
    );
    assert_eq!(
        requests[1]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("compaction")
    );
    assert_eq!(
        requests[2]
            .headers
            .get("x-stainless-helper")
            .and_then(|v| v.to_str().ok()),
        Some("BetaToolRunner")
    );

    let compaction_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(compaction_body["model"], "claude-3-5-haiku-latest");
    assert_eq!(
        compaction_body["messages"][1]["content"][0]["text"],
        "Summarize the conversation."
    );

    let final_body: serde_json::Value = serde_json::from_slice(&requests[2].body).unwrap();
    assert_eq!(final_body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(final_body["messages"][0]["role"], "user");
    assert_eq!(
        final_body["messages"][0]["content"][0]["text"],
        "<summary>compressed</summary>"
    );
}

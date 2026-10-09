// Maps to: TS lib/tools/ToolRunner.ts (BetaToolRunner)

use std::collections::{HashMap, HashSet};
use std::future::{Future, IntoFuture};
use std::pin::Pin;

use crate::RequestOptions;
use crate::client::Anthropic;
use crate::core::error::ApiError;
use crate::resources::beta::messages as beta_messages;
use crate::resources::messages::*;

use super::compaction_control::{
    CompactionControl, DEFAULT_SUMMARY_PROMPT, DEFAULT_TOKEN_THRESHOLD,
};
use super::tool_error::ToolError;

// ─────────────────────────────────────────────────────────────────────────────
// RunnableTool trait
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaRunnableTool -- a tool that can be executed
///
/// Implement this trait for each tool you want the model to be able to call.
/// The runner will match tool-use blocks by [`name()`](RunnableTool::name),
/// optionally validate/transform input with [`parse()`](RunnableTool::parse),
/// and pass the parsed JSON to [`run()`](RunnableTool::run).
#[async_trait::async_trait]
pub trait RunnableTool: Send + Sync {
    /// The name the model uses to invoke this tool. Must match the name in
    /// [`definition()`](RunnableTool::definition).
    fn name(&self) -> &str;

    /// Optional SDK-helper marker to include in the `x-stainless-helper`
    /// request header when this tool is used by [`BetaToolRunner`].
    ///
    /// Maps to TS helper-created runnable tools that carry
    /// `SDK_HELPER_SYMBOL`, such as `mcpTool`.
    fn stainless_helper(&self) -> Option<&str> {
        None
    }

    /// Returns the JSON tool definition sent to the API (the `Tool` schema).
    fn definition(&self) -> serde_json::Value;

    /// Parse and optionally transform raw JSON input before execution.
    ///
    /// Maps to TS `BetaRunnableTool.parse`. The default implementation is the
    /// identity transform, so existing tools only need to override it when they
    /// require validation or deserialization before [`run()`](RunnableTool::run).
    fn parse(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        Ok(input)
    }

    /// Execute the tool with the given parsed input and return beta tool-result
    /// content.
    ///
    /// Maps to TS `BetaRunnableTool.run`, which may return either text or an
    /// array of beta `tool_result` content blocks. The default implementation
    /// preserves the original Rust text-only runner API by wrapping
    /// [`run()`](RunnableTool::run) output as `BetaToolResultContent::Text`.
    async fn run_beta_tool_result_content(
        &self,
        input: serde_json::Value,
    ) -> Result<beta_messages::BetaToolResultContent, ToolError> {
        self.run(input)
            .await
            .map(beta_messages::BetaToolResultContent::Text)
    }

    /// Execute the tool with the given parsed input and return a text result,
    /// or a [`ToolError`] on failure.
    async fn run(&self, input: serde_json::Value) -> Result<String, ToolError>;
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRunnerParams
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaToolRunnerParams -- parameters for creating a ToolRunner
///
/// Bundles the message-create params, the set of runnable tools, and an
/// optional iteration cap.
pub struct ToolRunnerParams {
    /// The base parameters sent to the messages API on each iteration.
    pub create_params: MessageCreateParams,
    /// The runnable tools available for execution.
    pub tools: Vec<Box<dyn RunnableTool>>,
    /// Maximum number of API-call iterations. `None` and `Some(0)` are
    /// unbounded, matching TS's truthy `max_iterations` guard.
    pub max_iterations: Option<usize>,
}

fn max_iterations_reached(max_iterations: Option<usize>, iteration_count: usize) -> bool {
    matches!(max_iterations, Some(limit) if limit != 0 && iteration_count >= limit)
}

// ─────────────────────────────────────────────────────────────────────────────
// ToolRunner
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS BetaToolRunner -- iterative tool execution loop
///
/// Drives the agentic loop: send a message request, collect any `tool_use`
/// blocks from the response, execute the corresponding [`RunnableTool`]s,
/// append the results, and repeat until the model stops requesting tools or
/// the iteration cap is reached.
pub struct ToolRunner<'a> {
    client: &'a Anthropic,
    params: ToolRunnerParams,
}

impl<'a> ToolRunner<'a> {
    /// Create a new `ToolRunner` bound to the given client and parameters.
    pub fn new(client: &'a Anthropic, params: ToolRunnerParams) -> Self {
        ToolRunner { client, params }
    }

    /// Maps to: TS BetaToolRunner.runUntilDone()
    ///
    /// Runs the tool loop:
    /// 1. Send the current messages to the API.
    /// 2. Append the assistant response to the conversation.
    /// 3. Find all `tool_use` content blocks.
    /// 4. If none, return the response.
    /// 5. Execute each tool (or produce a "not found" error result).
    /// 6. Append tool results as a user message.
    /// 7. If `stop_reason` is not `ToolUse`, return the response.
    /// 8. Go to step 1 (up to `max_iterations`).
    ///
    /// When `max_iterations` is exhausted, return the last model response,
    /// matching TS `ToolRunner.runUntilDone()`.
    pub async fn run_until_done(&mut self) -> Result<Message, ApiError> {
        let mut iteration_count = 0usize;
        let mut last_message = None;

        loop {
            if max_iterations_reached(self.params.max_iterations, iteration_count) {
                break;
            }
            iteration_count += 1;

            let message = self
                .client
                .messages()
                .create(&self.params.create_params)
                .await?;

            // Append the assistant response to the conversation history.
            self.params.create_params.messages.push(MessageParam {
                role: "assistant".to_string(),
                content: MessageContent::Blocks(
                    message.content.iter().map(content_block_to_param).collect(),
                ),
            });

            // Collect all tool_use blocks from the response.
            let tool_uses: Vec<&ContentBlock> = message
                .content
                .iter()
                .filter(|block| matches!(block, ContentBlock::ToolUse { .. }))
                .collect();

            // No tool calls -- the model is done.
            if tool_uses.is_empty() {
                return Ok(message);
            }

            // Execute each requested tool and build result blocks.
            let mut tool_results = Vec::with_capacity(tool_uses.len());
            for tool_use in &tool_uses {
                let (tu_id, tu_name, tu_input) = match tool_use {
                    ContentBlock::ToolUse { id, name, input } => (id, name, input),
                    // The filter above guarantees only ToolUse variants reach here.
                    _ => continue,
                };

                let tool = self.params.tools.iter().find(|t| t.name() == tu_name);

                let result_block = match tool {
                    Some(tool) => match tool.run(tu_input.clone()).await {
                        Ok(output) => ContentBlockParam::ToolResult(ToolResultBlockParam {
                            tool_use_id: tu_id.clone(),
                            content: Some(ToolResultContent::Text(output)),
                            is_error: None,
                            cache_control: None,
                        }),
                        Err(e) => ContentBlockParam::ToolResult(ToolResultBlockParam {
                            tool_use_id: tu_id.clone(),
                            content: Some(stable_tool_error_content(e)),
                            is_error: Some(true),
                            cache_control: None,
                        }),
                    },
                    None => ContentBlockParam::ToolResult(ToolResultBlockParam {
                        tool_use_id: tu_id.clone(),
                        content: Some(ToolResultContent::Text(format!(
                            "Error: Tool '{}' not found",
                            tu_name
                        ))),
                        is_error: Some(true),
                        cache_control: None,
                    }),
                };

                tool_results.push(result_block);
            }

            // Append tool results as a user message.
            self.params.create_params.messages.push(MessageParam {
                role: "user".to_string(),
                content: MessageContent::Blocks(tool_results),
            });

            // If the model stopped for a reason other than tool_use, we are done.
            if message.stop_reason != Some(StopReason::ToolUse) {
                return Ok(message);
            }

            last_message = Some(message);
        }

        last_message.ok_or_else(|| {
            ApiError::Sdk("ToolRunner concluded without a message from the server".to_owned())
        })
    }

    /// TS-style camelCase alias for [`ToolRunner::run_until_done`].
    #[allow(non_snake_case)]
    pub async fn runUntilDone(&mut self) -> Result<Message, ApiError> {
        self.run_until_done().await
    }

    /// Returns a shared reference to the current conversation parameters.
    pub fn params(&self) -> &ToolRunnerParams {
        &self.params
    }

    /// Returns a mutable reference to the current conversation parameters,
    /// allowing callers to mutate messages or settings between iterations
    /// (e.g. for compaction or prompt injection).
    pub fn params_mut(&mut self) -> &mut ToolRunnerParams {
        &mut self.params
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BetaToolRunnerParams / BetaToolRunner
// ─────────────────────────────────────────────────────────────────────────────

/// Maps to: TS `BetaToolRunnerParams`.
///
/// Rust keeps message-create params separate from executable Rust tool
/// handlers. The runner uses the beta Messages API and beta request/response
/// types throughout the loop.
pub struct BetaToolRunnerParams {
    /// The beta message-create parameters sent on each iteration.
    pub create_params: beta_messages::BetaMessageCreateParams,
    /// Runnable tools available for execution.
    pub tools: Vec<Box<dyn RunnableTool>>,
    /// Maximum number of API-call iterations. `None` and `Some(0)` are
    /// unbounded, matching TS's truthy `max_iterations` guard.
    pub max_iterations: Option<usize>,
    /// Optional automatic compaction configuration.
    ///
    /// Maps to TS `BetaToolRunnerParams.compactionControl`.
    pub compaction_control: Option<CompactionControl>,
}

/// Rust equivalent of TS `BetaToolRunnerRequestOptions`.
pub type BetaToolRunnerRequestOptions = RequestOptions;

/// Minimal beta-message create interface used by [`BetaToolRunner`].
///
/// The TS runner is bound to a `Messages` resource and therefore inherits
/// provider overrides such as Bedrock/Vertex request rewriting. Rust models
/// that with this small trait so provider crates can supply their own beta
/// message sender while the default constructor continues to use the core
/// `Anthropic::beta().messages()` resource.
pub trait BetaMessageCreateClient: Send + Sync {
    /// Create a beta message with optional per-request options.
    fn create_beta_message_with_options<'b>(
        &'b self,
        params: &'b beta_messages::BetaMessageCreateParams,
        options: Option<&'b RequestOptions>,
    ) -> futures::future::BoxFuture<'b, Result<beta_messages::BetaMessage, ApiError>>;
}

struct CoreBetaMessageCreateClient<'a> {
    client: &'a Anthropic,
}

impl BetaMessageCreateClient for CoreBetaMessageCreateClient<'_> {
    fn create_beta_message_with_options<'b>(
        &'b self,
        params: &'b beta_messages::BetaMessageCreateParams,
        options: Option<&'b RequestOptions>,
    ) -> futures::future::BoxFuture<'b, Result<beta_messages::BetaMessage, ApiError>> {
        Box::pin(async move {
            self.client
                .beta()
                .messages()
                .create_with_options(params, options)
                .await
        })
    }
}

/// Maps to: TS `BetaToolRunner`.
///
/// Minimal async beta tool-execution loop for Rust. It mirrors the existing
/// stable [`ToolRunner`] while preserving beta message params, beta content
/// blocks, beta stop reasons, and beta message responses.
pub struct BetaToolRunner<'a> {
    message_client: Box<dyn BetaMessageCreateClient + 'a>,
    params: BetaToolRunnerParams,
    request_options: Option<RequestOptions>,
    completed_message: Option<beta_messages::BetaMessage>,
    tool_response_cache: tokio::sync::Mutex<Option<Option<beta_messages::BetaMessageParam>>>,
}

impl<'a> BetaToolRunner<'a> {
    /// Create a new beta tool runner bound to the given core client.
    pub fn new(client: &'a Anthropic, params: BetaToolRunnerParams) -> Self {
        Self::new_with_message_client(CoreBetaMessageCreateClient { client }, params)
    }

    /// Create a new beta tool runner bound to a custom beta-message sender.
    ///
    /// Provider crates use this to preserve TS provider behavior where
    /// `toolRunner()` goes through provider-specific `Messages.create()`
    /// request rewriting/auth hooks.
    pub fn new_with_message_client(
        message_client: impl BetaMessageCreateClient + 'a,
        params: BetaToolRunnerParams,
    ) -> Self {
        Self {
            message_client: Box::new(message_client),
            params,
            request_options: None,
            completed_message: None,
            tool_response_cache: tokio::sync::Mutex::new(None),
        }
    }

    /// Create a new beta tool runner with request options applied to normal
    /// tool-loop message requests.
    ///
    /// Maps to the optional `options` parameter accepted by TS
    /// `Messages.toolRunner(params, options)`.
    pub fn new_with_options(
        client: &'a Anthropic,
        params: BetaToolRunnerParams,
        request_options: RequestOptions,
    ) -> Self {
        Self::new_with_message_client_and_options(
            CoreBetaMessageCreateClient { client },
            params,
            request_options,
        )
    }

    /// Create a new beta tool runner with a custom beta-message sender and
    /// per-request options.
    pub fn new_with_message_client_and_options(
        message_client: impl BetaMessageCreateClient + 'a,
        params: BetaToolRunnerParams,
        request_options: RequestOptions,
    ) -> Self {
        Self {
            message_client: Box::new(message_client),
            params,
            request_options: Some(request_options),
            completed_message: None,
            tool_response_cache: tokio::sync::Mutex::new(None),
        }
    }

    /// Maps to: TS `BetaToolRunner.runUntilDone()`.
    pub async fn run_until_done(&mut self) -> Result<beta_messages::BetaMessage, ApiError> {
        if let Some(message) = &self.completed_message {
            return Ok(message.clone());
        }

        ensure_beta_tool_definitions(&mut self.params)?;
        let mut iteration_count = 0usize;
        let mut last_message = None;

        loop {
            if max_iterations_reached(self.params.max_iterations, iteration_count) {
                break;
            }
            iteration_count += 1;

            self.clear_tool_response_cache();
            let options =
                beta_tool_runner_request_options(&self.params, self.request_options.as_ref());
            let message = self
                .message_client
                .create_beta_message_with_options(&self.params.create_params, Some(&options))
                .await?;

            if self.check_and_compact(&message).await? {
                last_message = Some(message);
                continue;
            }

            let assistant_blocks = message
                .content
                .iter()
                .map(beta_content_block_to_param)
                .collect::<Result<Vec<_>, _>>()?;
            let assistant_message = beta_messages::BetaMessageParam {
                role: "assistant".to_owned(),
                content: beta_messages::BetaMessageContent::Blocks(assistant_blocks),
            };
            self.params
                .create_params
                .messages
                .push(assistant_message.clone());

            let Some(tool_message) =
                generate_beta_tool_response(&self.params, &assistant_message).await?
            else {
                return Ok(self.remember_completion(message));
            };

            self.params.create_params.messages.push(tool_message);

            if message.stop_reason != Some(beta_messages::BetaStopReason::ToolUse) {
                return Ok(self.remember_completion(message));
            }

            last_message = Some(message);
        }

        let message = last_message.ok_or_else(|| {
            ApiError::Sdk("ToolRunner concluded without a message from the server".to_owned())
        })?;
        Ok(self.remember_completion(message))
    }

    fn remember_completion(
        &mut self,
        message: beta_messages::BetaMessage,
    ) -> beta_messages::BetaMessage {
        self.completed_message = Some(message.clone());
        message
    }

    fn clear_tool_response_cache(&mut self) {
        *self.tool_response_cache.get_mut() = None;
    }

    /// TS-style camelCase alias for [`BetaToolRunner::run_until_done`].
    #[allow(non_snake_case)]
    pub async fn runUntilDone(&mut self) -> Result<beta_messages::BetaMessage, ApiError> {
        self.run_until_done().await
    }

    /// Rust eager equivalent of TS `BetaToolRunner.done()`.
    ///
    /// The TS runner is an async iterator and `done()` waits for the existing
    /// iterator completion. Rust's runner is eager, so this delegates to
    /// [`run_until_done`](Self::run_until_done) and returns the cached final
    /// message on subsequent calls.
    pub async fn done(&mut self) -> Result<beta_messages::BetaMessage, ApiError> {
        self.run_until_done().await
    }

    /// Checks token usage and performs automatic conversation compaction when
    /// `compaction_control` is enabled.
    ///
    /// Maps to TS `BetaToolRunner.#checkAndCompact()`.
    async fn check_and_compact(
        &mut self,
        message: &beta_messages::BetaMessage,
    ) -> Result<bool, ApiError> {
        let Some(compaction_control) = &self.params.compaction_control else {
            return Ok(false);
        };
        if !compaction_control.enabled {
            return Ok(false);
        }

        let tokens_used = non_negative_i64(message.usage.input_tokens)
            + non_negative_i64(message.usage.cache_creation_input_tokens.unwrap_or(0))
            + non_negative_i64(message.usage.cache_read_input_tokens.unwrap_or(0))
            + non_negative_i64(message.usage.output_tokens);
        let threshold = compaction_control
            .context_token_threshold
            .unwrap_or(DEFAULT_TOKEN_THRESHOLD);
        if tokens_used < threshold {
            return Ok(false);
        }

        let model = compaction_control
            .model
            .clone()
            .unwrap_or_else(|| self.params.create_params.model.clone());
        let summary_prompt = compaction_control
            .summary_prompt
            .clone()
            .unwrap_or_else(|| DEFAULT_SUMMARY_PROMPT.to_owned());

        strip_trailing_tool_use_for_compaction(&mut self.params.create_params.messages);

        let mut messages = self.params.create_params.messages.clone();
        messages.push(beta_messages::BetaMessageParam {
            role: "user".to_owned(),
            content: beta_messages::BetaMessageContent::Blocks(vec![
                beta_messages::BetaContentBlockParam::Text(beta_messages::BetaTextBlockParam {
                    text: summary_prompt,
                    stainless_helpers: Vec::new(),
                    cache_control: None,
                    citations: None,
                }),
            ]),
        });

        let compaction_params = beta_messages::BetaMessageCreateParams {
            model,
            messages,
            max_tokens: self.params.create_params.max_tokens,
            ..Default::default()
        };
        let mut headers = HashMap::new();
        headers.insert(
            "x-stainless-helper".to_owned(),
            Some("compaction".to_owned()),
        );
        let options = RequestOptions {
            headers: Some(headers),
            ..Default::default()
        };
        let response = self
            .message_client
            .create_beta_message_with_options(&compaction_params, Some(&options))
            .await?;

        let Some(beta_messages::BetaContentBlock::Text { .. }) = response.content.first() else {
            return Err(ApiError::Sdk(
                "Expected text response for compaction".to_owned(),
            ));
        };

        self.params.create_params.messages = vec![beta_messages::BetaMessageParam {
            role: "user".to_owned(),
            content: beta_messages::BetaMessageContent::Blocks(
                response
                    .content
                    .iter()
                    .map(beta_content_block_to_param)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        }];

        Ok(true)
    }

    /// Maps to: TS `BetaToolRunner.generateToolResponse()`.
    ///
    /// Generates a beta user message containing `tool_result` blocks for the
    /// given assistant message, or `None` when no beta `tool_use` blocks are
    /// present.
    pub async fn generate_tool_response(
        &self,
        last_message: &beta_messages::BetaMessageParam,
    ) -> Result<Option<beta_messages::BetaMessageParam>, ApiError> {
        {
            let cached = self.tool_response_cache.lock().await;
            if let Some(response) = &*cached {
                return Ok(response.clone());
            }
        }

        let response = generate_beta_tool_response(&self.params, last_message).await?;
        *self.tool_response_cache.lock().await = Some(response.clone());
        Ok(response)
    }

    /// TS-style camelCase alias for [`BetaToolRunner::generate_tool_response`].
    #[allow(non_snake_case)]
    pub async fn generateToolResponse(
        &self,
        last_message: &beta_messages::BetaMessageParam,
    ) -> Result<Option<beta_messages::BetaMessageParam>, ApiError> {
        self.generate_tool_response(last_message).await
    }

    /// Maps to: TS `BetaToolRunner.pushMessages()`.
    pub fn push_messages(&mut self, messages: Vec<beta_messages::BetaMessageParam>) {
        self.completed_message = None;
        self.clear_tool_response_cache();
        self.params.create_params.messages.extend(messages);
    }

    /// TS-style camelCase alias for [`BetaToolRunner::push_messages`].
    #[allow(non_snake_case)]
    pub fn pushMessages(&mut self, messages: Vec<beta_messages::BetaMessageParam>) {
        self.push_messages(messages);
    }

    /// Maps to: TS `BetaToolRunner.setMessagesParams()`.
    pub fn set_messages_params(&mut self, create_params: beta_messages::BetaMessageCreateParams) {
        self.completed_message = None;
        self.clear_tool_response_cache();
        self.params.create_params = create_params;
    }

    /// Rust equivalent of TS `setMessagesParams((prev) => next)` overload.
    pub fn set_messages_params_with<F>(&mut self, mutator: F)
    where
        F: FnOnce(
            &beta_messages::BetaMessageCreateParams,
        ) -> beta_messages::BetaMessageCreateParams,
    {
        let next = mutator(&self.params.create_params);
        self.set_messages_params(next);
    }

    /// TS-style camelCase alias for [`BetaToolRunner::set_messages_params`].
    #[allow(non_snake_case)]
    pub fn setMessagesParams(&mut self, create_params: beta_messages::BetaMessageCreateParams) {
        self.set_messages_params(create_params);
    }

    /// TS-style camelCase alias for [`BetaToolRunner::set_messages_params_with`].
    #[allow(non_snake_case)]
    pub fn setMessagesParamsWith<F>(&mut self, mutator: F)
    where
        F: FnOnce(
            &beta_messages::BetaMessageCreateParams,
        ) -> beta_messages::BetaMessageCreateParams,
    {
        self.set_messages_params_with(mutator);
    }

    /// Returns a shared reference to the current beta runner params.
    pub fn params(&self) -> &BetaToolRunnerParams {
        &self.params
    }

    /// Returns a mutable reference to the current beta runner params.
    pub fn params_mut(&mut self) -> &mut BetaToolRunnerParams {
        self.completed_message = None;
        self.clear_tool_response_cache();
        &mut self.params
    }
}

impl<'a> IntoFuture for BetaToolRunner<'a> {
    type Output = Result<beta_messages::BetaMessage, ApiError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + 'a>>;

    /// Consuming Rust equivalent of TS `await runner` / `runner.then(...)`.
    fn into_future(mut self) -> Self::IntoFuture {
        Box::pin(async move { self.run_until_done().await })
    }
}

impl<'runner, 'client> IntoFuture for &'runner mut BetaToolRunner<'client>
where
    'client: 'runner,
{
    type Output = Result<beta_messages::BetaMessage, ApiError>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + 'runner>>;

    /// Borrowing Rust equivalent of TS `await runner` that preserves the runner.
    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.run_until_done().await })
    }
}

fn non_negative_i64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn strip_trailing_tool_use_for_compaction(messages: &mut Vec<beta_messages::BetaMessageParam>) {
    let remove_last = match messages.last_mut() {
        Some(last) if last.role == "assistant" => match &mut last.content {
            beta_messages::BetaMessageContent::Blocks(blocks) => {
                blocks.retain(|block| {
                    !matches!(block, beta_messages::BetaContentBlockParam::ToolUse(_))
                });
                blocks.is_empty()
            }
            beta_messages::BetaMessageContent::Text(_) => false,
        },
        _ => false,
    };

    if remove_last {
        messages.pop();
    }
}

fn beta_tool_runner_request_options(
    params: &BetaToolRunnerParams,
    user_options: Option<&RequestOptions>,
) -> RequestOptions {
    let mut helpers = vec!["BetaToolRunner".to_owned()];
    let mut seen = HashSet::from(["BetaToolRunner".to_owned()]);

    for tool in &params.tools {
        if let Some(helper) = tool.stainless_helper() {
            if seen.insert(helper.to_owned()) {
                helpers.push(helper.to_owned());
            }
        }
    }

    for message in &params.create_params.messages {
        if let beta_messages::BetaMessageContent::Blocks(blocks) = &message.content {
            for block in blocks {
                for helper in beta_tool_runner_block_stainless_helpers(block) {
                    if seen.insert(helper.clone()) {
                        helpers.push(helper.clone());
                    }
                }
            }
        }
    }

    let mut options = user_options.cloned().unwrap_or_default();
    let mut headers = HashMap::new();
    headers.insert("x-stainless-helper".to_owned(), Some(helpers.join(", ")));

    if let Some(user_headers) = options.headers.take() {
        for (key, value) in user_headers {
            headers.insert(key.to_lowercase(), value);
        }
    }

    options.headers = Some(headers);
    options
}

fn beta_tool_runner_block_stainless_helpers(
    block: &beta_messages::BetaContentBlockParam,
) -> &[String] {
    match block {
        beta_messages::BetaContentBlockParam::Text(block) => &block.stainless_helpers,
        beta_messages::BetaContentBlockParam::Image(block) => &block.stainless_helpers,
        beta_messages::BetaContentBlockParam::Document(block) => &block.stainless_helpers,
        _ => &[],
    }
}

async fn generate_beta_tool_response(
    params: &BetaToolRunnerParams,
    last_message: &beta_messages::BetaMessageParam,
) -> Result<Option<beta_messages::BetaMessageParam>, ApiError> {
    if last_message.role != "assistant" {
        return Ok(None);
    }

    let beta_messages::BetaMessageContent::Blocks(blocks) = &last_message.content else {
        return Ok(None);
    };

    let tool_uses: Vec<_> = blocks
        .iter()
        .filter_map(|block| match block {
            beta_messages::BetaContentBlockParam::ToolUse(tool_use) => Some(tool_use),
            _ => None,
        })
        .collect();

    if tool_uses.is_empty() {
        return Ok(None);
    }

    let mut tool_results = Vec::with_capacity(tool_uses.len());
    for tool_use in tool_uses {
        let tool = params
            .tools
            .iter()
            .find(|tool| tool.name() == tool_use.name)
            .map(|tool| tool.as_ref());
        tool_results.push(
            beta_tool_result_block(&tool_use.id, &tool_use.name, &tool_use.input, tool).await,
        );
    }

    Ok(Some(beta_messages::BetaMessageParam {
        role: "user".to_owned(),
        content: beta_messages::BetaMessageContent::Blocks(tool_results),
    }))
}

async fn beta_tool_result_block(
    tool_use_id: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool: Option<&dyn RunnableTool>,
) -> beta_messages::BetaContentBlockParam {
    match tool {
        Some(tool) => {
            let result = match tool.parse(tool_input.clone()) {
                Ok(parsed) => tool.run_beta_tool_result_content(parsed).await,
                Err(err) => Err(err),
            };

            match result {
                Ok(output) => beta_messages::BetaContentBlockParam::ToolResult(
                    beta_messages::BetaToolResultBlockParam {
                        tool_use_id: tool_use_id.to_owned(),
                        cache_control: None,
                        content: Some(output),
                        is_error: None,
                    },
                ),
                Err(e) => beta_messages::BetaContentBlockParam::ToolResult(
                    beta_messages::BetaToolResultBlockParam {
                        tool_use_id: tool_use_id.to_owned(),
                        cache_control: None,
                        content: Some(beta_tool_error_content(e)),
                        is_error: Some(true),
                    },
                ),
            }
        }
        None => beta_messages::BetaContentBlockParam::ToolResult(
            beta_messages::BetaToolResultBlockParam {
                tool_use_id: tool_use_id.to_owned(),
                cache_control: None,
                content: Some(beta_messages::BetaToolResultContent::Text(format!(
                    "Error: Tool '{}' not found",
                    tool_name
                ))),
                is_error: Some(true),
            },
        ),
    }
}

fn stable_tool_error_content(error: ToolError) -> ToolResultContent {
    if let Some(blocks) = error.content_blocks {
        let json_blocks = blocks
            .into_iter()
            .filter_map(|block| serde_json::to_value(block).ok())
            .collect();
        return ToolResultContent::Blocks(json_blocks);
    }

    ToolResultContent::Text(error.content.unwrap_or(error.message))
}

fn beta_tool_error_content(error: ToolError) -> beta_messages::BetaToolResultContent {
    if let Some(blocks) = error.content_blocks {
        return beta_messages::BetaToolResultContent::Blocks(blocks);
    }

    beta_messages::BetaToolResultContent::Text(error.content.unwrap_or(error.message))
}

fn ensure_beta_tool_definitions(params: &mut BetaToolRunnerParams) -> Result<(), ApiError> {
    let existing_tools = params.create_params.tools.get_or_insert_with(Vec::new);
    let mut existing_names: HashSet<String> =
        existing_tools.iter().filter_map(beta_tool_name).collect();

    for runnable in &params.tools {
        if !existing_names.insert(runnable.name().to_owned()) {
            continue;
        }

        let definition = runnable.definition();
        let tool =
            serde_json::from_value::<beta_messages::BetaToolUnion>(definition).map_err(|err| {
                ApiError::Sdk(format!(
                    "failed to convert runnable tool '{}' definition into beta tool params: {err}",
                    runnable.name()
                ))
            })?;
        existing_tools.push(tool);
    }

    Ok(())
}

fn beta_tool_name(tool: &beta_messages::BetaToolUnion) -> Option<String> {
    serde_json::to_value(tool).ok().and_then(|value| {
        value
            .get("name")
            .or_else(|| value.get("mcp_server_name"))
            .and_then(|name| name.as_str())
            .map(str::to_owned)
    })
}

fn beta_content_block_to_param(
    block: &beta_messages::BetaContentBlock,
) -> Result<beta_messages::BetaContentBlockParam, ApiError> {
    let value = serde_json::to_value(block).map_err(|err| {
        ApiError::Sdk(format!(
            "failed to serialize beta content block for tool runner: {err}"
        ))
    })?;
    serde_json::from_value(value).map_err(|err| {
        ApiError::Sdk(format!(
            "failed to convert beta content block for tool runner request history: {err}"
        ))
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// text_citation_to_param
// ─────────────────────────────────────────────────────────────────────────────

/// Convert a response-side `TextCitation` to a request-side `TextCitationParam`,
/// stripping the `file_id` field that is only present on response types.
fn text_citation_to_param(citation: &TextCitation) -> TextCitationParam {
    match citation {
        TextCitation::CharLocation {
            cited_text,
            document_index,
            document_title,
            end_char_index,
            start_char_index,
            ..
        } => TextCitationParam::CharLocation {
            cited_text: cited_text.clone(),
            document_index: *document_index,
            document_title: document_title.clone(),
            end_char_index: *end_char_index,
            start_char_index: *start_char_index,
        },
        TextCitation::PageLocation {
            cited_text,
            document_index,
            document_title,
            end_page_number,
            start_page_number,
            ..
        } => TextCitationParam::PageLocation {
            cited_text: cited_text.clone(),
            document_index: *document_index,
            document_title: document_title.clone(),
            end_page_number: *end_page_number,
            start_page_number: *start_page_number,
        },
        TextCitation::ContentBlockLocation {
            cited_text,
            document_index,
            document_title,
            end_block_index,
            start_block_index,
            ..
        } => TextCitationParam::ContentBlockLocation {
            cited_text: cited_text.clone(),
            document_index: *document_index,
            document_title: document_title.clone(),
            end_block_index: *end_block_index,
            start_block_index: *start_block_index,
        },
        TextCitation::WebSearchResultLocation {
            cited_text,
            encrypted_index,
            title,
            url,
        } => TextCitationParam::WebSearchResultLocation {
            cited_text: cited_text.clone(),
            encrypted_index: encrypted_index.clone(),
            title: title.clone(),
            url: url.clone(),
        },
        TextCitation::SearchResultLocation {
            cited_text,
            end_block_index,
            search_result_index,
            source,
            start_block_index,
            title,
        } => TextCitationParam::SearchResultLocation {
            cited_text: cited_text.clone(),
            end_block_index: *end_block_index,
            search_result_index: *search_result_index,
            source: source.clone(),
            start_block_index: *start_block_index,
            title: title.clone(),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// content_block_to_param
// ─────────────────────────────────────────────────────────────────────────────

fn web_search_result_content_to_param(
    content: &WebSearchToolResultBlockContent,
) -> WebSearchToolResultBlockParamContent {
    match content {
        WebSearchToolResultBlockContent::Error(error) => {
            WebSearchToolResultBlockParamContent::Error(WebSearchToolRequestError {
                error_code: error.error_code.clone(),
                type_name: error.type_name.clone(),
            })
        }
        WebSearchToolResultBlockContent::Results(results) => {
            WebSearchToolResultBlockParamContent::Results(
                results
                    .iter()
                    .map(|result| WebSearchResultBlockParam {
                        encrypted_content: result.encrypted_content.clone(),
                        title: result.title.clone(),
                        type_name: result.type_name.clone(),
                        url: result.url.clone(),
                        page_age: result.page_age.clone(),
                    })
                    .collect(),
            )
        }
    }
}

/// Maps to: TS helper -- convert a response `ContentBlock` to a
/// `ContentBlockParam` suitable for re-sending in the conversation history.
fn content_block_to_param(block: &ContentBlock) -> ContentBlockParam {
    match block {
        ContentBlock::Text { citations, text } => ContentBlockParam::Text(TextBlockParam {
            text: text.clone(),
            cache_control: None,
            citations: citations
                .as_ref()
                .map(|c| c.iter().map(text_citation_to_param).collect()),
            type_name: None,
        }),

        ContentBlock::ToolUse { id, input, name } => {
            ContentBlockParam::ToolUse(ToolUseBlockParam {
                id: id.clone(),
                input: input.clone(),
                name: name.clone(),
                cache_control: None,
            })
        }

        ContentBlock::Thinking {
            signature,
            thinking,
        } => ContentBlockParam::Thinking(ThinkingBlockParam {
            signature: signature.clone(),
            thinking: thinking.clone(),
        }),

        ContentBlock::RedactedThinking { data } => {
            ContentBlockParam::RedactedThinking(RedactedThinkingBlockParam { data: data.clone() })
        }

        ContentBlock::ServerToolUse { id, input, name } => {
            ContentBlockParam::ServerToolUse(ServerToolUseBlockParam {
                id: id.clone(),
                input: input.clone(),
                name: name.clone(),
                cache_control: None,
            })
        }

        ContentBlock::WebSearchToolResult {
            content,
            tool_use_id,
        } => ContentBlockParam::WebSearchToolResult(WebSearchToolResultBlockParam {
            content: web_search_result_content_to_param(content),
            tool_use_id: tool_use_id.clone(),
            cache_control: None,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientOptions;

    // ── Wiremock-based integration tests for ToolRunner ──────────────────

    /// Helper: build an `Anthropic` client pointing at the given base URL.
    fn make_client(base_url: &str) -> Anthropic {
        Anthropic::new(ClientOptions {
            api_key: "test-key".into(),
            base_url: Some(base_url.to_string()),
            max_retries: Some(0),
            ..Default::default()
        })
        .expect("client construction should succeed")
    }

    /// Helper: build a `Message` response with the given content blocks and
    /// stop reason.
    fn make_response_message(content: Vec<ContentBlock>, stop_reason: StopReason) -> Message {
        Message {
            id: "msg_test".to_string(),
            request_id: None,
            content,
            model: "claude-sonnet-4-20250514".to_string(),
            role: "assistant".to_string(),
            stop_reason: Some(stop_reason),
            stop_sequence: None,
            type_name: "message".to_string(),
            usage: Usage {
                cache_creation: None,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
                inference_geo: None,
                input_tokens: 10,
                output_tokens: 20,
                server_tool_use: None,
                service_tier: None,
            },
        }
    }

    /// Helper: build default `MessageCreateParams`.
    fn make_create_params(user_text: &str) -> MessageCreateParams {
        MessageCreateParams {
            max_tokens: 1024,
            messages: vec![MessageParam {
                role: "user".to_string(),
                content: MessageContent::Text(user_text.to_string()),
            }],
            model: "claude-sonnet-4-20250514".to_string(),
            inference_geo: None,
            metadata: None,
            output_config: None,
            service_tier: None,
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

    /// A simple weather tool for testing.
    struct WeatherTool;

    #[async_trait::async_trait]
    impl RunnableTool for WeatherTool {
        fn name(&self) -> &str {
            "getWeather"
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({
                "name": "getWeather",
                "description": "Get weather for a location",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "location": { "type": "string" }
                    }
                }
            })
        }

        async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
            let location = input["location"].as_str().unwrap_or("unknown");
            Ok(format!("Sunny in {}", location))
        }
    }

    /// A calculator tool for multi-tool testing.
    struct CalculatorTool;

    #[async_trait::async_trait]
    impl RunnableTool for CalculatorTool {
        fn name(&self) -> &str {
            "calculate"
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({
                "name": "calculate",
                "description": "Perform calculations",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "a": { "type": "number" },
                        "b": { "type": "number" },
                        "operation": { "type": "string" }
                    }
                }
            })
        }

        async fn run(&self, input: serde_json::Value) -> Result<String, ToolError> {
            let a = input["a"].as_f64().unwrap_or(0.0);
            let b = input["b"].as_f64().unwrap_or(0.0);
            let op = input["operation"].as_str().unwrap_or("");
            match op {
                "add" => Ok(format!("{}", a + b)),
                "multiply" => Ok(format!("{}", a * b)),
                _ => Err(ToolError::new(format!("Unknown operation: {}", op))),
            }
        }
    }

    /// A tool that always fails, for error-handling tests.
    struct FailingTool;

    #[async_trait::async_trait]
    impl RunnableTool for FailingTool {
        fn name(&self) -> &str {
            "failingTool"
        }

        fn definition(&self) -> serde_json::Value {
            serde_json::json!({
                "name": "failingTool",
                "description": "A tool that always fails",
                "input_schema": {
                    "type": "object",
                    "properties": {}
                }
            })
        }

        async fn run(&self, _input: serde_json::Value) -> Result<String, ToolError> {
            Err(ToolError::new("Tool execution failed"))
        }
    }

    // -- TS-parity: yields message when no tools requested ----------------

    #[tokio::test]
    async fn tool_runner_yields_message_when_no_tools_requested() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // The model returns a text-only response (end_turn), no tool_use.
        let response_msg = make_response_message(
            vec![ContentBlock::Text {
                citations: None,
                text: "The weather is nice today.".to_string(),
            }],
            StopReason::EndTurn,
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_msg))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = make_client(&mock_server.uri());
        let params = ToolRunnerParams {
            create_params: make_create_params("What is the weather?"),
            tools: vec![Box::new(WeatherTool)],
            max_iterations: None,
        };
        let mut runner = ToolRunner::new(&client, params);
        let result = runner.run_until_done().await.expect("should succeed");

        // The model returned text only -- result should be the same message.
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "The weather is nice today.");
            }
            _ => panic!("expected Text content block"),
        }
    }

    // -- TS-parity: executes multiple tools --------------------------------

    #[tokio::test]
    async fn tool_runner_executes_multiple_tools() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // First response: model requests two tools.
        let tool_use_msg = make_response_message(
            vec![
                ContentBlock::ToolUse {
                    id: "tool_1".to_string(),
                    name: "getWeather".to_string(),
                    input: serde_json::json!({"location": "NYC"}),
                },
                ContentBlock::ToolUse {
                    id: "tool_2".to_string(),
                    name: "calculate".to_string(),
                    input: serde_json::json!({"a": 2, "b": 3, "operation": "add"}),
                },
            ],
            StopReason::ToolUse,
        );

        // Second response: model returns text.
        let final_msg = make_response_message(
            vec![ContentBlock::Text {
                citations: None,
                text: "NYC is sunny and 2+3=5.".to_string(),
            }],
            StopReason::EndTurn,
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&tool_use_msg))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&final_msg))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = make_client(&mock_server.uri());
        let params = ToolRunnerParams {
            create_params: make_create_params("Get weather and calculate 2+3"),
            tools: vec![Box::new(WeatherTool), Box::new(CalculatorTool)],
            max_iterations: None,
        };
        let mut runner = ToolRunner::new(&client, params);
        let result = runner.run_until_done().await.expect("should succeed");

        // Final message is the text response.
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "NYC is sunny and 2+3=5.");
            }
            _ => panic!("expected Text content block"),
        }

        // Verify conversation history has the tool results.
        let messages = &runner.params().create_params.messages;
        // user -> assistant(tool_use) -> user(tool_results) -> assistant(text)
        assert_eq!(messages.len(), 4);

        // Check that tool results were appended as user message.
        let tool_results_msg = &messages[2];
        assert_eq!(tool_results_msg.role, "user");
        if let MessageContent::Blocks(blocks) = &tool_results_msg.content {
            assert_eq!(blocks.len(), 2);
            // First result: weather
            match &blocks[0] {
                ContentBlockParam::ToolResult(tr) => {
                    assert_eq!(tr.tool_use_id, "tool_1");
                    match &tr.content {
                        Some(ToolResultContent::Text(t)) => assert_eq!(t, "Sunny in NYC"),
                        _ => panic!("expected text tool result"),
                    }
                    assert!(tr.is_error.is_none());
                }
                _ => panic!("expected ToolResult"),
            }
            // Second result: calculator
            match &blocks[1] {
                ContentBlockParam::ToolResult(tr) => {
                    assert_eq!(tr.tool_use_id, "tool_2");
                    match &tr.content {
                        Some(ToolResultContent::Text(t)) => assert_eq!(t, "5"),
                        _ => panic!("expected text tool result"),
                    }
                    assert!(tr.is_error.is_none());
                }
                _ => panic!("expected ToolResult"),
            }
        } else {
            panic!("expected Blocks content");
        }
    }

    // -- TS-parity: handles missing tool (error result) --------------------

    #[tokio::test]
    async fn tool_runner_handles_missing_tool() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Model requests a tool that doesn't exist.
        let tool_use_msg = make_response_message(
            vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "unknownTool".to_string(),
                input: serde_json::json!({"param": "value"}),
            }],
            StopReason::ToolUse,
        );

        let final_msg = make_response_message(
            vec![ContentBlock::Text {
                citations: None,
                text: "I could not find that tool.".to_string(),
            }],
            StopReason::EndTurn,
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&tool_use_msg))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&final_msg))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = make_client(&mock_server.uri());
        let params = ToolRunnerParams {
            create_params: make_create_params("Use a tool"),
            tools: vec![Box::new(WeatherTool)], // Only weather, not unknownTool
            max_iterations: None,
        };
        let mut runner = ToolRunner::new(&client, params);
        let result = runner.run_until_done().await.expect("should succeed");

        // Should still return successfully.
        match &result.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "I could not find that tool.");
            }
            _ => panic!("expected Text content block"),
        }

        // Verify the error result was sent back.
        let messages = &runner.params().create_params.messages;
        let tool_results_msg = &messages[2];
        if let MessageContent::Blocks(blocks) = &tool_results_msg.content {
            match &blocks[0] {
                ContentBlockParam::ToolResult(tr) => {
                    assert_eq!(tr.tool_use_id, "tool_1");
                    assert_eq!(tr.is_error, Some(true));
                    match &tr.content {
                        Some(ToolResultContent::Text(t)) => {
                            assert!(t.contains("unknownTool"));
                            assert!(t.contains("not found"));
                        }
                        _ => panic!("expected text error content"),
                    }
                }
                _ => panic!("expected ToolResult"),
            }
        } else {
            panic!("expected Blocks content");
        }
    }

    // -- TS-parity: handles tool execution error ---------------------------

    #[tokio::test]
    async fn tool_runner_handles_tool_execution_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Model requests a tool that will fail.
        let tool_use_msg = make_response_message(
            vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "failingTool".to_string(),
                input: serde_json::json!({}),
            }],
            StopReason::ToolUse,
        );

        let final_msg = make_response_message(
            vec![ContentBlock::Text {
                citations: None,
                text: "The tool failed, but I can continue.".to_string(),
            }],
            StopReason::EndTurn,
        );

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&tool_use_msg))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&final_msg))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = make_client(&mock_server.uri());
        let params = ToolRunnerParams {
            create_params: make_create_params("Run the failing tool"),
            tools: vec![Box::new(FailingTool)],
            max_iterations: None,
        };
        let mut runner = ToolRunner::new(&client, params);
        let result = runner.run_until_done().await.expect("should succeed");

        match &result.content[0] {
            ContentBlock::Text { text, .. } => {
                assert_eq!(text, "The tool failed, but I can continue.");
            }
            _ => panic!("expected Text content block"),
        }

        // Verify the error result was sent back with is_error = true.
        let messages = &runner.params().create_params.messages;
        let tool_results_msg = &messages[2];
        if let MessageContent::Blocks(blocks) = &tool_results_msg.content {
            match &blocks[0] {
                ContentBlockParam::ToolResult(tr) => {
                    assert_eq!(tr.tool_use_id, "tool_1");
                    assert_eq!(tr.is_error, Some(true));
                    match &tr.content {
                        Some(ToolResultContent::Text(t)) => {
                            assert!(t.contains("Tool execution failed"));
                        }
                        _ => panic!("expected text error content"),
                    }
                }
                _ => panic!("expected ToolResult"),
            }
        } else {
            panic!("expected Blocks content");
        }
    }

    // -- TS-parity: respects max_iterations limit --------------------------

    #[tokio::test]
    async fn tool_runner_respects_max_iterations_limit() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // The model keeps requesting tools every iteration. With max_iterations=2
        // the loop runs twice, then returns the last model response like TS.
        let tool_use_msg1 = make_response_message(
            vec![ContentBlock::ToolUse {
                id: "tool_1".to_string(),
                name: "getWeather".to_string(),
                input: serde_json::json!({"location": "Paris"}),
            }],
            StopReason::ToolUse,
        );

        let tool_use_msg2 = make_response_message(
            vec![ContentBlock::ToolUse {
                id: "tool_2".to_string(),
                name: "getWeather".to_string(),
                input: serde_json::json!({"location": "Berlin"}),
            }],
            StopReason::ToolUse,
        );

        // We expect exactly 2 calls: max_iterations caps API requests; TS does
        // not make an extra final request after the limit is reached.
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&tool_use_msg1))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&tool_use_msg2))
            .up_to_n_times(1)
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = make_client(&mock_server.uri());
        let params = ToolRunnerParams {
            create_params: make_create_params("Use tools repeatedly"),
            tools: vec![Box::new(WeatherTool)],
            max_iterations: Some(2),
        };
        let mut runner = ToolRunner::new(&client, params);
        let result = runner.run_until_done().await.expect("should succeed");

        // The returned message is the second model response, which still asks
        // for a tool; no third request is made after max_iterations is reached.
        match &result.content[0] {
            ContentBlock::ToolUse { id, .. } => assert_eq!(id, "tool_2"),
            _ => panic!("expected ToolUse content block"),
        }

        // Conversation should have: user + 2*(assistant+user) = 5 messages.
        let messages = &runner.params().create_params.messages;
        assert_eq!(messages.len(), 5);
    }

    // ── content_block_to_param tests ─────────────────────────────────────

    #[test]
    fn content_block_text_round_trips() {
        let block = ContentBlock::Text {
            citations: None,
            text: "hello".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::Text(t) => {
                assert_eq!(t.text, "hello");
                assert!(t.cache_control.is_none());
                assert!(t.citations.is_none());
            }
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn content_block_tool_use_round_trips() {
        let block = ContentBlock::ToolUse {
            id: "tu_123".to_string(),
            input: serde_json::json!({"key": "value"}),
            name: "my_tool".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::ToolUse(tu) => {
                assert_eq!(tu.id, "tu_123");
                assert_eq!(tu.name, "my_tool");
                assert_eq!(tu.input, serde_json::json!({"key": "value"}));
            }
            _ => panic!("expected ToolUse variant"),
        }
    }

    #[test]
    fn content_block_thinking_round_trips() {
        let block = ContentBlock::Thinking {
            signature: "sig".to_string(),
            thinking: "hmm".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::Thinking(t) => {
                assert_eq!(t.signature, "sig");
                assert_eq!(t.thinking, "hmm");
            }
            _ => panic!("expected Thinking variant"),
        }
    }

    #[test]
    fn content_block_redacted_thinking_round_trips() {
        let block = ContentBlock::RedactedThinking {
            data: "redacted_blob".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::RedactedThinking(r) => {
                assert_eq!(r.data, "redacted_blob");
            }
            _ => panic!("expected RedactedThinking variant"),
        }
    }

    #[test]
    fn content_block_server_tool_use_round_trips() {
        let block = ContentBlock::ServerToolUse {
            id: "stu_1".to_string(),
            input: serde_json::json!({}),
            name: "web_search".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::ServerToolUse(s) => {
                assert_eq!(s.id, "stu_1");
                assert_eq!(s.name, "web_search");
            }
            _ => panic!("expected ServerToolUse variant"),
        }
    }

    #[test]
    fn content_block_web_search_result_round_trips() {
        let block = ContentBlock::WebSearchToolResult {
            content: WebSearchToolResultBlockContent::Results(vec![WebSearchResultBlock {
                encrypted_content: "enc".to_string(),
                page_age: None,
                title: "Result".to_string(),
                type_name: "web_search_result".to_string(),
                url: "https://example.com".to_string(),
            }]),
            tool_use_id: "wsr_1".to_string(),
        };
        let param = content_block_to_param(&block);
        match param {
            ContentBlockParam::WebSearchToolResult(w) => {
                assert_eq!(w.tool_use_id, "wsr_1");
                match &w.content {
                    WebSearchToolResultBlockParamContent::Results(r) => {
                        assert_eq!(r.len(), 1);
                        assert_eq!(r[0].url, "https://example.com");
                    }
                    _ => panic!("expected Results variant"),
                }
            }
            _ => panic!("expected WebSearchToolResult variant"),
        }
    }
}

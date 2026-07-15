// Integration/parity tests for beta message-specific typed models.
//
// These tests intentionally live under tests/ instead of inline unit-test
// modules so migrated TS SDK parity coverage stays separate from library code.

use std::any::TypeId;

use anthropic_sdk::resources::beta::messages::batches::{BetaBatchCreateParams, BetaBatchRequest};
use anthropic_sdk::resources::beta::messages::types as beta_messages;
use anthropic_sdk::resources::beta::messages::types::{
    BetaClearToolInputs, BetaCodeExecutionTool20250825, BetaCodeExecutionToolResultBlockContent,
    BetaContainer, BetaContainerParam, BetaContentBlock, BetaContentBlockDelta,
    BetaContentBlockParam, BetaContextManagementAppliedEdit, BetaContextManagementConfig,
    BetaContextManagementEdit, BetaContextManagementResponse, BetaContextManagementTrigger,
    BetaImageBlockParam, BetaImageSource, BetaInputTokensThreshold, BetaMCPServerDefinition,
    BetaMCPToolDefaultConfig, BetaMCPToolset, BetaMemoryTool20250818, BetaMessageContent,
    BetaMessageParam, BetaMessageStreamEvent, BetaRequestMCPServerToolConfiguration,
    BetaServerToolName, BetaSkillParams, BetaThinkingKeep, BetaToolAllowedCaller, BetaToolCaller,
    BetaToolComputerUse20251124, BetaToolReferenceBlockParam, BetaToolResultBlockParam,
    BetaToolResultContent, BetaToolResultContentBlockParam, BetaToolSearchToolBm2520251119,
    BetaToolSearchToolRegex20251119, BetaToolTextEditor20250728, BetaToolUnion, BetaToolUsesCount,
    BetaWebFetchTool20250910, BetaWebFetchToolResultBlockContent, BetaWebSearchTool20250305,
    BetaWebSearchToolUserLocation,
};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

#[test]
fn beta_message_raw_json_slots_are_intentional_ts_unknown_or_internal_body_transforms() {
    let source = include_str!("../../../../src/resources/beta/messages/types.rs");
    for (idx, line) in source.lines().enumerate() {
        if !line.contains("serde_json::Value") {
            continue;
        }
        let allowed = line.contains("pub type BetaUnknown = serde_json::Value")
            || line.contains("body: &mut serde_json::Value")
            || line.contains("serde_json::Value::Bool")
            || line.contains("serde_json::Value::Object");
        assert!(
            allowed,
            "unexpected raw serde_json::Value in beta messages public types at line {}: {}",
            idx + 1,
            line.trim()
        );
    }
}

#[test]
fn beta_unprefixed_message_aliases_match_ts_namespace_exports() {
    assert_same_type::<beta_messages::Messages<'static>, beta_messages::BetaMessages<'static>>();
    assert_same_type::<beta_messages::MessageCreateParams, beta_messages::BetaMessageCreateParams>(
    );
    assert_same_type::<
        beta_messages::MessageCreateParamsBase,
        beta_messages::BetaMessageCreateParams,
    >();
    assert_same_type::<
        beta_messages::MessageCreateParamsNonStreaming,
        beta_messages::BetaMessageCreateParams,
    >();
    assert_same_type::<
        beta_messages::MessageCreateParamsStreaming,
        beta_messages::BetaMessageCreateParams,
    >();
    assert_same_type::<
        beta_messages::MessageCountTokensParams,
        beta_messages::BetaMessageCountTokensParams,
    >();
}

#[test]
fn beta_prefixed_shared_aliases_match_ts_exported_names() {
    let cache: beta_messages::BetaCacheControlEphemeral = Default::default();
    assert_eq!(cache.type_name, "ephemeral");

    let creation = beta_messages::BetaCacheCreation {
        ephemeral_1h_input_tokens: 1,
        ephemeral_5m_input_tokens: 2,
    };
    assert_eq!(creation.ephemeral_1h_input_tokens, 1);

    let format: beta_messages::BetaJSONOutputFormat = Default::default();
    let output = beta_messages::BetaOutputConfig {
        effort: None,
        format: Some(format),
    };
    assert!(output.format.is_some());

    let metadata = beta_messages::BetaMetadata {
        user_id: Some("user-123".to_owned()),
    };
    assert_eq!(metadata.user_id.as_deref(), Some("user-123"));

    let _: beta_messages::BetaMessageStreamParams =
        beta_messages::BetaMessageCreateParams::default();
    let _: beta_messages::BetaThinkingConfigParam =
        beta_messages::BetaThinkingConfigParam::Adaptive;
    let _: beta_messages::BetaTextDelta = beta_messages::BetaContentBlockDelta::TextDelta {
        text: "hello".to_owned(),
    };
    let citation = beta_messages::BetaTextCitation::SearchResultLocation {
        cited_text: "quoted".to_owned(),
        end_block_index: 2,
        search_result_index: 0,
        source: "search".to_owned(),
        start_block_index: 1,
        title: Some("result".to_owned()),
    };
    let _: beta_messages::BetaCitationsDelta =
        anthropic_sdk::resources::messages::CitationsDeltaData {
            type_name: "citations_delta".to_owned(),
            citation,
        };
    let _: beta_messages::BetaBase64ImageSource = beta_messages::BetaImageSource::Base64 {
        data: "AAAA".to_owned(),
        media_type: "image/png".to_owned(),
    };
    let _: beta_messages::BetaCodeExecutionResultBlockParam =
        beta_messages::BetaCodeExecutionResultBlock {
            content: vec![beta_messages::BetaCodeExecutionOutputBlock {
                file_id: "file_123".to_owned(),
                type_name: "code_execution_output".to_owned(),
            }],
            return_code: 0,
            stderr: String::new(),
            stdout: "ok".to_owned(),
            type_name: "code_execution_result".to_owned(),
        };
    let _: beta_messages::BetaToolChoice = beta_messages::BetaToolChoice::None;
    let _: beta_messages::BetaToolChoiceAny = beta_messages::BetaToolChoiceAny::Any {
        disable_parallel_tool_use: Some(true),
    };
    assert_same_type::<beta_messages::InputSchema, beta_messages::BetaToolInputSchema>();
    let _: beta_messages::BetaRawMessageDeltaEventDelta = beta_messages::BetaMessageDelta {
        container: None,
        stop_reason: None,
        stop_sequence: None,
    };
    let _: beta_messages::Delta = beta_messages::BetaMessageDelta {
        container: None,
        stop_reason: None,
        stop_sequence: None,
    };
    assert_same_type::<
        beta_messages::BetaWebSearchTool20250305UserLocation,
        beta_messages::BetaWebSearchToolUserLocation,
    >();
    assert_same_type::<beta_messages::UserLocation, beta_messages::BetaWebSearchToolUserLocation>();

    let json = serde_json::to_value(beta_messages::BetaWebSearchToolResultBlockContent::Results(
        vec![beta_messages::BetaWebSearchResultBlock {
            encrypted_content: "enc".to_owned(),
            page_age: None,
            title: "title".to_owned(),
            type_name: "web_search_result".to_owned(),
            url: "https://example.com".to_owned(),
        }],
    ))
    .unwrap();
    assert_eq!(json[0]["type"], "web_search_result");
}

#[test]
fn context_management_config_serializes_typed_edits() {
    let cfg = BetaContextManagementConfig {
        edits: Some(vec![
            BetaContextManagementEdit::ClearToolUses20250919 {
                clear_at_least: Some(BetaInputTokensThreshold::input_tokens(1000)),
                clear_tool_inputs: Some(BetaClearToolInputs::ToolNames(vec![
                    "get_weather".to_owned()
                ])),
                exclude_tools: None,
                keep: Some(BetaToolUsesCount::tool_uses(2)),
                trigger: Some(BetaContextManagementTrigger::InputTokens(
                    BetaInputTokensThreshold::input_tokens(150_000),
                )),
            },
            BetaContextManagementEdit::ClearThinking20251015 {
                keep: Some(BetaThinkingKeep::all()),
            },
            BetaContextManagementEdit::Compact20260112 {
                instructions: Some("summarize concisely".to_owned()),
                pause_after_compaction: Some(true),
                trigger: Some(BetaInputTokensThreshold::input_tokens(150_000)),
            },
        ]),
    };

    let json = serde_json::to_value(cfg).unwrap();
    assert_eq!(json["edits"][0]["type"], "clear_tool_uses_20250919");
    assert_eq!(json["edits"][0]["clear_at_least"]["type"], "input_tokens");
    assert_eq!(json["edits"][1]["keep"], "all");
    assert_eq!(json["edits"][2]["type"], "compact_20260112");
}

#[test]
fn context_management_response_deserializes_typed_applied_edits() {
    let response: BetaContextManagementResponse = serde_json::from_value(serde_json::json!({
        "applied_edits": [
            {
                "type": "clear_tool_uses_20250919",
                "cleared_input_tokens": 42,
                "cleared_tool_uses": 3
            },
            {
                "type": "clear_thinking_20251015",
                "cleared_input_tokens": 11,
                "cleared_thinking_turns": 2
            }
        ]
    }))
    .unwrap();

    assert_eq!(response.applied_edits.len(), 2);
    match &response.applied_edits[0] {
        BetaContextManagementAppliedEdit::ClearToolUses20250919 {
            cleared_input_tokens,
            cleared_tool_uses,
        } => {
            assert_eq!(*cleared_input_tokens, 42);
            assert_eq!(*cleared_tool_uses, 3);
        }
        other => panic!("unexpected edit: {other:?}"),
    }
}

#[test]
fn mcp_server_definition_uses_typed_tool_configuration() {
    let server = BetaMCPServerDefinition {
        name: "weather".to_owned(),
        type_name: "url".to_owned(),
        url: "https://example.com/mcp".to_owned(),
        authorization_token: None,
        tool_configuration: Some(BetaRequestMCPServerToolConfiguration {
            allowed_tools: Some(vec!["get_weather".to_owned()]),
            enabled: Some(true),
        }),
    };

    let json = serde_json::to_value(server).unwrap();
    assert_eq!(json["type"], "url");
    assert_eq!(
        json["tool_configuration"]["allowed_tools"][0],
        "get_weather"
    );
    assert_eq!(json["tool_configuration"]["enabled"], true);
}

#[test]
fn container_skills_are_typed_for_response_and_params() {
    let container: BetaContainer = serde_json::from_value(serde_json::json!({
        "id": "container_123",
        "expires_at": "2026-01-01T00:00:00Z",
        "skills": [{"skill_id": "skill_123", "type": "custom", "version": "latest"}]
    }))
    .unwrap();
    assert_eq!(container.skills.unwrap()[0].skill_id, "skill_123");

    let param = BetaContainerParam::Params {
        id: Some("container_123".to_owned()),
        skills: Some(vec![BetaSkillParams {
            skill_id: "skill_123".to_owned(),
            type_name: "custom".to_owned(),
            version: None,
        }]),
    };
    let json = serde_json::to_value(param).unwrap();
    assert_eq!(json["skills"][0]["type"], "custom");
    assert!(json["skills"][0].get("version").is_none());
}

#[test]
fn beta_tool_union_serializes_memory_code_execution_web_fetch_and_mcp_toolset() {
    let tools = vec![
        BetaToolUnion::Memory20250818(BetaMemoryTool20250818::default()),
        BetaToolUnion::CodeExecution20250825(BetaCodeExecutionTool20250825 {
            allowed_callers: Some(vec![BetaToolAllowedCaller::Direct]),
            ..Default::default()
        }),
        BetaToolUnion::WebSearch20250305(BetaWebSearchTool20250305 {
            user_location: Some(BetaWebSearchToolUserLocation {
                city: Some("San Francisco".to_owned()),
                country: Some("US".to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        BetaToolUnion::WebFetch20250910(BetaWebFetchTool20250910 {
            allowed_domains: Some(vec!["example.com".to_owned()]),
            max_uses: Some(2),
            ..Default::default()
        }),
        BetaToolUnion::ComputerUse20251124(BetaToolComputerUse20251124 {
            display_height_px: 768,
            display_width_px: 1024,
            name: "computer".to_owned(),
            type_name: "computer_20251124".to_owned(),
            allowed_callers: None,
            cache_control: None,
            defer_loading: None,
            display_number: Some(0),
            enable_zoom: Some(true),
            input_examples: None,
            strict: None,
        }),
        BetaToolUnion::TextEditor20250728(BetaToolTextEditor20250728 {
            max_characters: Some(4096),
            ..Default::default()
        }),
        BetaToolUnion::ToolSearchBm2520251119(BetaToolSearchToolBm2520251119::default()),
        BetaToolUnion::ToolSearchRegex20251119(BetaToolSearchToolRegex20251119::default()),
        BetaToolUnion::McpToolset(BetaMCPToolset {
            mcp_server_name: "weather".to_owned(),
            type_name: "mcp_toolset".to_owned(),
            cache_control: None,
            configs: None,
            default_config: Some(BetaMCPToolDefaultConfig {
                defer_loading: Some(true),
                enabled: Some(true),
            }),
        }),
    ];

    let json = serde_json::to_value(tools).unwrap();
    assert_eq!(json[0]["type"], "memory_20250818");
    assert_eq!(json[1]["type"], "code_execution_20250825");
    assert_eq!(json[1]["allowed_callers"][0], "direct");
    assert_eq!(json[2]["type"], "web_search_20250305");
    assert_eq!(json[2]["user_location"]["type"], "approximate");
    assert_eq!(json[3]["type"], "web_fetch_20250910");
    assert_eq!(json[3]["allowed_domains"][0], "example.com");
    assert_eq!(json[4]["type"], "computer_20251124");
    assert_eq!(json[4]["enable_zoom"], true);
    assert_eq!(json[5]["type"], "text_editor_20250728");
    assert_eq!(json[6]["type"], "tool_search_tool_bm25_20251119");
    assert_eq!(json[7]["type"], "tool_search_tool_regex_20251119");
    assert_eq!(json[8]["type"], "mcp_toolset");
    assert_eq!(json[8]["default_config"]["defer_loading"], true);
}

#[test]
fn beta_batch_requests_use_typed_message_create_params() {
    let batch = BetaBatchCreateParams {
        betas: Some(vec!["custom-beta".to_owned()]),
        requests: vec![BetaBatchRequest {
            custom_id: "request-1".to_owned(),
            params: anthropic_sdk::resources::beta::messages::types::BetaMessageCreateParams {
                max_tokens: 64,
                messages: vec![BetaMessageParam {
                    role: "user".to_owned(),
                    content: BetaMessageContent::Text("hello".to_owned()),
                }],
                model: "claude-opus-4-6".to_owned(),
                betas: Some(vec!["nested-beta-is-header-only".to_owned()]),
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
            },
        }],
    };

    let json = serde_json::to_value(batch).unwrap();
    assert_eq!(json["requests"][0]["params"]["max_tokens"], 64);
    assert_eq!(
        json["requests"][0]["params"]["messages"][0]["content"],
        "hello"
    );
    assert!(json.get("betas").is_none());
    assert!(json["requests"][0]["params"].get("betas").is_none());
}

#[test]
fn beta_message_params_support_beta_request_content_blocks() {
    let message = BetaMessageParam {
        role: "user".to_owned(),
        content: BetaMessageContent::Blocks(vec![
            BetaContentBlockParam::Image(BetaImageBlockParam {
                source: BetaImageSource::File {
                    file_id: "file_123".to_owned(),
                },
                stainless_helpers: Vec::new(),
                cache_control: None,
            }),
            BetaContentBlockParam::ToolResult(BetaToolResultBlockParam {
                tool_use_id: "toolu_123".to_owned(),
                cache_control: None,
                content: Some(BetaToolResultContent::Blocks(vec![
                    BetaToolResultContentBlockParam::ToolReference(BetaToolReferenceBlockParam {
                        tool_name: "memory".to_owned(),
                        cache_control: None,
                    }),
                ])),
                is_error: Some(false),
            }),
        ]),
    };

    let json = serde_json::to_value(message).unwrap();
    assert_eq!(json["content"][0]["type"], "image");
    assert_eq!(json["content"][0]["source"]["type"], "file");
    assert_eq!(json["content"][0]["source"]["file_id"], "file_123");
    assert_eq!(json["content"][1]["type"], "tool_result");
    assert_eq!(json["content"][1]["content"][0]["type"], "tool_reference");
    assert_eq!(json["content"][1]["content"][0]["tool_name"], "memory");
}

#[test]
fn beta_content_blocks_deserialize_typed_server_tools_and_results() {
    let server: BetaContentBlock = serde_json::from_value(serde_json::json!({
        "type": "server_tool_use",
        "id": "srvu_123",
        "name": "web_fetch",
        "input": {"url": "https://example.com"},
        "caller": {"type": "code_execution_20250825", "tool_id": "toolu_123"}
    }))
    .unwrap();

    match server {
        BetaContentBlock::ServerToolUse {
            caller,
            input,
            name,
            ..
        } => {
            assert_eq!(name, BetaServerToolName::WebFetch);
            assert_eq!(input["url"], "https://example.com");
            match caller.unwrap() {
                BetaToolCaller::CodeExecution20250825 { tool_id } => {
                    assert_eq!(tool_id, "toolu_123")
                }
                other => panic!("unexpected caller: {other:?}"),
            }
        }
        other => panic!("unexpected server tool block: {other:?}"),
    }

    let code_result: BetaContentBlock = serde_json::from_value(serde_json::json!({
        "type": "code_execution_tool_result",
        "tool_use_id": "toolu_456",
        "content": {
            "type": "code_execution_result",
            "content": [{"type": "code_execution_output", "file_id": "file_123"}],
            "return_code": 0,
            "stderr": "",
            "stdout": "ok"
        }
    }))
    .unwrap();

    match code_result {
        BetaContentBlock::CodeExecutionToolResult {
            content,
            tool_use_id,
        } => {
            assert_eq!(tool_use_id, "toolu_456");
            match content {
                BetaCodeExecutionToolResultBlockContent::Result(result) => {
                    assert_eq!(result.stdout, "ok");
                    assert_eq!(result.content[0].file_id, "file_123");
                }
                other => panic!("unexpected code execution content: {other:?}"),
            }
        }
        other => panic!("unexpected code execution block: {other:?}"),
    }

    let fetch_error: BetaContentBlock = serde_json::from_value(serde_json::json!({
        "type": "web_fetch_tool_result",
        "tool_use_id": "toolu_789",
        "content": {"type": "web_fetch_tool_result_error", "error_code": "url_not_allowed"}
    }))
    .unwrap();

    match fetch_error {
        BetaContentBlock::WebFetchToolResult { content, .. } => match content {
            BetaWebFetchToolResultBlockContent::Error(error) => {
                assert_eq!(
                    serde_json::to_value(error.error_code).unwrap(),
                    "url_not_allowed"
                );
            }
            other => panic!("unexpected web fetch content: {other:?}"),
        },
        other => panic!("unexpected web fetch block: {other:?}"),
    }
}

#[test]
fn beta_stream_events_use_typed_deltas_and_usage() {
    let delta: BetaMessageStreamEvent = serde_json::from_value(serde_json::json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "compaction_delta", "content": "summary"}
    }))
    .unwrap();

    match delta {
        BetaMessageStreamEvent::ContentBlockDelta {
            delta: BetaContentBlockDelta::CompactionDelta { content },
            index,
        } => {
            assert_eq!(index, 0);
            assert_eq!(content.as_deref(), Some("summary"));
        }
        other => panic!("unexpected content block delta event: {other:?}"),
    }

    let message_delta: BetaMessageStreamEvent = serde_json::from_value(serde_json::json!({
        "type": "message_delta",
        "delta": {
            "container": null,
            "stop_reason": "compaction",
            "stop_sequence": null
        },
        "usage": {
            "cache_creation_input_tokens": null,
            "cache_read_input_tokens": null,
            "input_tokens": null,
            "iterations": [{
                "type": "compaction",
                "cache_creation": null,
                "cache_creation_input_tokens": 10,
                "cache_read_input_tokens": 0,
                "input_tokens": 20,
                "output_tokens": 5
            }],
            "output_tokens": 7,
            "server_tool_use": {"web_fetch_requests": 1, "web_search_requests": 2}
        },
        "context_management": null
    }))
    .unwrap();

    match message_delta {
        BetaMessageStreamEvent::MessageDelta { delta, usage, .. } => {
            assert_eq!(
                serde_json::to_value(delta.stop_reason).unwrap(),
                "compaction"
            );
            assert_eq!(usage.output_tokens, 7);
            assert_eq!(usage.iterations.unwrap().len(), 1);
            assert_eq!(usage.server_tool_use.unwrap().web_fetch_requests, 1);
        }
        other => panic!("unexpected message delta event: {other:?}"),
    }
}

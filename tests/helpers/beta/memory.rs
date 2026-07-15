// Ported from TS SDK: tests/helpers/beta/memory.test.ts-style behavior.
//
// The missing-handler error intentionally avoids stub-like wording so public
// Rust SDK surfaces do not look unfinished.

use anthropic_sdk::helpers::beta::memory::{
    betaMemoryTool, beta_memory_tool, BetaMemoryToolHandlers, MemoryToolHandlers,
};
use anthropic_sdk::resources::beta::messages::types::{
    BetaMemoryTool20250818Command, BetaMemoryTool20250818CreateCommand,
    BetaMemoryTool20250818ViewCommand, BetaTextBlockParam, BetaToolResultContent,
    BetaToolResultContentBlockParam,
};
use anthropic_sdk::sdk_lib::tools::{RunnableTool, ToolError};
use serde_json::json;

#[test]
fn beta_memory_command_serializes_and_deserializes_typed_union() {
    let command = BetaMemoryTool20250818Command::Create(BetaMemoryTool20250818CreateCommand {
        path: "/notes/todo.md".to_owned(),
        file_text: "remember this".to_owned(),
    });

    let json = serde_json::to_value(&command).unwrap();
    assert_eq!(json["command"], "create");
    assert_eq!(json["path"], "/notes/todo.md");

    let roundtrip: BetaMemoryTool20250818Command = serde_json::from_value(json).unwrap();
    assert_eq!(roundtrip.command_name(), "create");
}

#[tokio::test]
async fn beta_memory_tool_ts_style_aliases_are_available() {
    let handlers: MemoryToolHandlers = BetaMemoryToolHandlers::new()
        .with_view(|cmd: BetaMemoryTool20250818ViewCommand| Ok(format!("view {}", cmd.path)));
    let tool = betaMemoryTool(handlers);
    let result = tool
        .run(json!({"command": "view", "path": "/notes"}))
        .await
        .unwrap();
    assert_eq!(result, "view /notes");
}

#[tokio::test]
async fn beta_memory_tool_dispatches_to_typed_handler() {
    let tool = beta_memory_tool(BetaMemoryToolHandlers::new().with_view(
        |cmd: BetaMemoryTool20250818ViewCommand| {
            Ok(format!(
                "view {} {:?}",
                cmd.path,
                cmd.view_range.unwrap_or_default()
            ))
        },
    ));

    assert_eq!(tool.name(), "memory");
    let definition = tool.definition();
    assert_eq!(definition["type"], "memory_20250818");
    assert_eq!(definition["name"], "memory");

    let result = tool
        .run(json!({"command": "view", "path": "/notes", "view_range": [1, 5]}))
        .await
        .unwrap();
    assert_eq!(result, "view /notes [1, 5]");
}

#[tokio::test]
async fn beta_memory_tool_supports_structured_result_content_like_ts() {
    let tool = beta_memory_tool(BetaMemoryToolHandlers::new().with_view_content(|cmd| {
        Ok(BetaToolResultContent::Blocks(vec![
            BetaToolResultContentBlockParam::Text(BetaTextBlockParam {
                text: format!("viewed {}", cmd.path),
                stainless_helpers: Vec::new(),
                cache_control: None,
                citations: None,
            }),
        ]))
    }));

    let content = tool
        .run_beta_tool_result_content(json!({"command": "view", "path": "/notes"}))
        .await
        .unwrap();

    match content {
        BetaToolResultContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            let BetaToolResultContentBlockParam::Text(text) = &blocks[0] else {
                panic!("expected text block");
            };
            assert_eq!(text.text, "viewed /notes");
        }
        BetaToolResultContent::Text(text) => panic!("expected blocks, got {text}"),
    }
}

#[tokio::test]
async fn beta_memory_tool_reports_missing_handler_like_ts() {
    let tool = beta_memory_tool(BetaMemoryToolHandlers::new());

    let err = tool
        .run(json!({"command": "delete", "path": "/notes/todo.md"}))
        .await
        .expect_err("delete handler should be missing");

    assert_eq!(
        err.to_string(),
        "No handler configured for memory command `delete`"
    );
}

#[tokio::test]
async fn beta_memory_tool_propagates_handler_errors() {
    let tool = beta_memory_tool(
        BetaMemoryToolHandlers::new().with_create(|_| Err(ToolError::new("disk is read-only"))),
    );

    let err = tool
        .run(json!({"command": "create", "path": "/x", "file_text": "hi"}))
        .await
        .expect_err("handler should fail");

    assert_eq!(err.to_string(), "disk is read-only");
}

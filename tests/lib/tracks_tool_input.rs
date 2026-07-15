// Migrated from TS tests/lib/TracksToolInput.test.ts: ensure the public
// stream helper type covers every content-block variant whose input JSON is
// incrementally tracked during streaming.

use anthropic_sdk::resources::beta::messages::{
    BetaContentBlock, BetaServerToolName, BetaToolCaller,
};
use anthropic_sdk::resources::messages::{ContentBlock, WebSearchToolResultBlockContent};
use anthropic_sdk::sdk_lib::beta_message_stream as beta_stream;
use anthropic_sdk::sdk_lib::message_stream as stable_stream;

#[test]
fn stable_tracks_tool_input_accepts_only_blocks_with_input() {
    let tool = ContentBlock::ToolUse {
        id: "toolu_1".to_owned(),
        input: serde_json::json!({"city": "SF"}),
        name: "get_weather".to_owned(),
    };
    let tracked = stable_stream::tracks_tool_input(&tool).expect("tool_use is tracked");
    assert_eq!(tracked.id(), "toolu_1");
    assert_eq!(tracked.name(), "get_weather");
    assert_eq!(tracked.input(), &serde_json::json!({"city": "SF"}));

    let server_tool = ContentBlock::ServerToolUse {
        id: "srv_1".to_owned(),
        input: serde_json::json!({"query": "rust"}),
        name: "web_search".to_owned(),
    };
    assert!(matches!(
        stable_stream::TracksToolInput::try_from(&server_tool).unwrap(),
        stable_stream::TracksToolInput::ServerToolUse { .. }
    ));

    let text = ContentBlock::Text {
        citations: None,
        text: "hello".to_owned(),
    };
    assert!(stable_stream::tracks_tool_input(&text).is_none());

    let web_result = ContentBlock::WebSearchToolResult {
        content: WebSearchToolResultBlockContent::Results(vec![]),
        tool_use_id: "toolu_1".to_owned(),
    };
    assert!(stable_stream::TracksToolInput::try_from(&web_result).is_err());
}

#[test]
fn beta_tracks_tool_input_includes_tool_server_tool_and_mcp_inputs() {
    let tool = BetaContentBlock::ToolUse {
        id: "toolu_beta".to_owned(),
        input: serde_json::json!({"x": 1}),
        name: "custom_tool".to_owned(),
        caller: None,
    };
    assert!(matches!(
        beta_stream::tracks_tool_input(&tool).unwrap(),
        beta_stream::TracksToolInput::ToolUse { .. }
    ));

    let server_tool = BetaContentBlock::ServerToolUse {
        caller: Some(BetaToolCaller::Direct),
        id: "srv_beta".to_owned(),
        input: [("query".to_owned(), serde_json::json!("rust"))]
            .into_iter()
            .collect(),
        name: BetaServerToolName::WebSearch,
    };
    let tracked = beta_stream::TracksToolInput::try_from(&server_tool).unwrap();
    assert_eq!(tracked.id(), "srv_beta");
    assert_eq!(tracked.name_json(), serde_json::json!("web_search"));

    let mcp_tool = BetaContentBlock::McpToolUse {
        id: "mcp_1".to_owned(),
        input: serde_json::json!({"path": "/tmp/a"}),
        name: "read_file".to_owned(),
        server_name: "filesystem".to_owned(),
    };
    assert!(matches!(
        beta_stream::tracks_tool_input(&mcp_tool).unwrap(),
        beta_stream::TracksToolInput::McpToolUse { .. }
    ));

    let text = BetaContentBlock::Text {
        citations: None,
        text: "not tracked".to_owned(),
    };
    assert!(beta_stream::tracks_tool_input(&text).is_none());
}

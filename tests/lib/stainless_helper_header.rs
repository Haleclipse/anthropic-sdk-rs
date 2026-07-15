use anthropic_sdk::resources::messages::MessageParam;
use anthropic_sdk::sdk_lib::stainless_helper_header::*;

#[test]
fn sdk_helper_symbol_rust_equivalent_is_public() {
    assert_eq!(SDK_HELPER_SYMBOL, "__stainless_helper");
}

#[test]
fn was_created_by_helper_present() {
    let val = serde_json::json!({
        "__stainless_helper": "mcpTool",
        "name": "my_tool"
    });
    assert_eq!(was_created_by_stainless_helper(&val), Some("mcpTool"));
    assert_eq!(wasCreatedByStainlessHelper(&val), Some("mcpTool"));
}

#[test]
fn was_created_by_helper_absent() {
    let val = serde_json::json!({"name": "my_tool"});
    assert_eq!(was_created_by_stainless_helper(&val), None);
}

#[test]
fn was_created_by_helper_non_object() {
    let val = serde_json::json!("just a string");
    assert_eq!(was_created_by_stainless_helper(&val), None);
}

#[test]
fn collect_from_tools() {
    let tools = vec![
        serde_json::json!({"__stainless_helper": "mcpTool", "name": "a"}),
        serde_json::json!({"name": "b"}),
        serde_json::json!({"__stainless_helper": "zodTool", "name": "c"}),
    ];
    let messages: Vec<MessageParam> = vec![];
    let result = collect_stainless_helpers(Some(&tools), &messages);
    assert_eq!(result, vec!["mcpTool", "zodTool"]);
}

#[test]
fn collect_deduplicates() {
    let tools = vec![
        serde_json::json!({"__stainless_helper": "mcpTool", "name": "a"}),
        serde_json::json!({"__stainless_helper": "mcpTool", "name": "b"}),
    ];
    let messages: Vec<MessageParam> = vec![];
    let result = collect_stainless_helpers(Some(&tools), &messages);
    assert_eq!(result, vec!["mcpTool"]);
}

#[test]
fn stainless_helper_header_empty() {
    let messages: Vec<MessageParam> = vec![];
    let result = stainless_helper_header(None, &messages);
    assert!(result.is_empty());
}

#[test]
fn stainless_helper_header_with_tools() {
    let tools = vec![
        serde_json::json!({"__stainless_helper": "mcpTool", "name": "a"}),
        serde_json::json!({"__stainless_helper": "zodTool", "name": "b"}),
    ];
    let messages: Vec<MessageParam> = vec![];
    let result = stainless_helper_header(Some(&tools), &messages);
    assert_eq!(
        result.get("x-stainless-helper").map(|s| s.as_str()),
        Some("mcpTool, zodTool")
    );
}

#[test]
fn stainless_helper_header_from_file_present() {
    let file = serde_json::json!({"__stainless_helper": "fileUpload", "data": "..."});
    let result = stainless_helper_header_from_file(&file);
    assert_eq!(
        result.get("x-stainless-helper").map(|s| s.as_str()),
        Some("fileUpload")
    );
}

#[test]
fn stainless_helper_header_from_file_absent() {
    let file = serde_json::json!({"data": "..."});
    let result = stainless_helper_header_from_file(&file);
    assert!(result.is_empty());
}

#[test]
fn ts_style_camel_case_aliases() {
    let tools = vec![serde_json::json!({"__stainless_helper": "mcpTool", "name": "a"})];
    let messages: Vec<MessageParam> = vec![];
    assert_eq!(
        collectStainlessHelpers(Some(&tools), &messages),
        vec!["mcpTool"]
    );
    assert_eq!(
        stainlessHelperHeader(Some(&tools), &messages)
            .get("x-stainless-helper")
            .map(String::as_str),
        Some("mcpTool")
    );
    assert!(stainlessHelperHeaderFromFile(None).is_empty());
    assert_eq!(
        stainlessHelperHeaderFromFile(Some(&tools[0]))
            .get("x-stainless-helper")
            .map(String::as_str),
        Some("mcpTool")
    );
}

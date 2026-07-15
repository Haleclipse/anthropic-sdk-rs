// Maps to: TS lib/stainless-helper-header.ts
//
// Shared utilities for tracking SDK helper usage. Collects helper metadata
// from tools and messages to build the `x-stainless-helper` HTTP header.
//
// In the TypeScript SDK, helper objects are tagged with a `Symbol` property.
// In Rust we use a simple string tag stored in a well-known JSON field
// (`__stainless_helper`) since Rust values cannot carry hidden symbol
// properties.

use std::collections::HashMap;

use crate::resources::messages::MessageParam;

/// Well-known JSON object key used to mark values created by SDK helpers.
///
/// Internal JSON-object key used for Rust helper metadata.
const STAINLESS_HELPER_KEY: &str = "__stainless_helper";

/// Rust public equivalent of TS `SDK_HELPER_SYMBOL`.
///
/// TypeScript uses a non-enumerable `Symbol('anthropic.sdk.stainlessHelper')`;
/// Rust cannot attach hidden symbol properties to arbitrary values, so helper
/// metadata is represented by this well-known skipped/auxiliary key.
pub const SDK_HELPER_SYMBOL: &str = STAINLESS_HELPER_KEY;

/// Checks whether a JSON value was created by a stainless SDK helper by
/// looking for the well-known `__stainless_helper` key.
///
/// Maps to: TS `wasCreatedByStainlessHelper()`
pub fn was_created_by_stainless_helper(value: &serde_json::Value) -> Option<&str> {
    value
        .as_object()
        .and_then(|obj| obj.get(STAINLESS_HELPER_KEY))
        .and_then(|v| v.as_str())
}

/// TS-style camelCase alias for [`was_created_by_stainless_helper`].
#[allow(non_snake_case)]
pub fn wasCreatedByStainlessHelper(value: &serde_json::Value) -> Option<&str> {
    was_created_by_stainless_helper(value)
}

/// Collects helper names from tools and messages arrays.
/// Returns a deduplicated, sorted vector of helper names found.
///
/// Maps to: TS `collectStainlessHelpers()`
pub fn collect_stainless_helpers(
    tools: Option<&[serde_json::Value]>,
    messages: &[MessageParam],
) -> Vec<String> {
    let mut helpers: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Collect from tools
    if let Some(tool_list) = tools {
        for tool in tool_list {
            if let Some(name) = was_created_by_stainless_helper(tool) {
                if seen.insert(name.to_owned()) {
                    helpers.push(name.to_owned());
                }
            }
        }
    }

    // Collect from messages and their content blocks
    for message in messages {
        // Check the message-level JSON representation for the helper tag.
        // In Rust, MessageParam is a struct, so we serialize to check.
        if let Ok(msg_val) = serde_json::to_value(message) {
            if let Some(name) = was_created_by_stainless_helper(&msg_val) {
                if seen.insert(name.to_owned()) {
                    helpers.push(name.to_owned());
                }
            }

            // Check content blocks if content is an array
            if let Some(blocks) = msg_val.get("content").and_then(|c| c.as_array()) {
                for block in blocks {
                    if let Some(name) = was_created_by_stainless_helper(block) {
                        if seen.insert(name.to_owned()) {
                            helpers.push(name.to_owned());
                        }
                    }
                }
            }
        }
    }

    helpers
}

/// TS-style camelCase alias for [`collect_stainless_helpers`].
#[allow(non_snake_case)]
pub fn collectStainlessHelpers(
    tools: Option<&[serde_json::Value]>,
    messages: &[MessageParam],
) -> Vec<String> {
    collect_stainless_helpers(tools, messages)
}

/// Builds the `x-stainless-helper` header value from tools and messages.
/// Returns an empty map if no helpers are found.
///
/// Maps to: TS `stainlessHelperHeader()`
pub fn stainless_helper_header(
    tools: Option<&[serde_json::Value]>,
    messages: &[MessageParam],
) -> HashMap<String, String> {
    let helpers = collect_stainless_helpers(tools, messages);
    if helpers.is_empty() {
        return HashMap::new();
    }
    let mut headers = HashMap::with_capacity(1);
    headers.insert("x-stainless-helper".to_owned(), helpers.join(", "));
    headers
}

/// TS-style camelCase alias for [`stainless_helper_header`].
#[allow(non_snake_case)]
pub fn stainlessHelperHeader(
    tools: Option<&[serde_json::Value]>,
    messages: &[MessageParam],
) -> HashMap<String, String> {
    stainless_helper_header(tools, messages)
}

/// Builds the `x-stainless-helper` header value from a single JSON value
/// (e.g. a file object). Returns an empty map if the value is not marked
/// with a helper.
///
/// Maps to: TS `stainlessHelperHeaderFromFile()`
pub fn stainless_helper_header_from_file(file: &serde_json::Value) -> HashMap<String, String> {
    match was_created_by_stainless_helper(file) {
        Some(name) => {
            let mut headers = HashMap::with_capacity(1);
            headers.insert("x-stainless-helper".to_owned(), name.to_owned());
            headers
        }
        None => HashMap::new(),
    }
}

/// Optional-file variant mirroring TS `null`/`undefined` handling.
pub fn stainless_helper_header_from_optional_file(
    file: Option<&serde_json::Value>,
) -> HashMap<String, String> {
    file.map(stainless_helper_header_from_file)
        .unwrap_or_default()
}

/// TS-style camelCase alias for [`stainless_helper_header_from_optional_file`].
#[allow(non_snake_case)]
pub fn stainlessHelperHeaderFromFile(file: Option<&serde_json::Value>) -> HashMap<String, String> {
    stainless_helper_header_from_optional_file(file)
}

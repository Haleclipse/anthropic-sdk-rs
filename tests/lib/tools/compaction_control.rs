// Parity coverage for TS lib/tools/CompactionControl.ts

use anthropic_sdk::sdk_lib::tools::{
    CompactionControl, DEFAULT_SUMMARY_PROMPT, DEFAULT_TOKEN_THRESHOLD,
};

#[test]
fn compaction_control_defaults_match_ts_constants() {
    assert_eq!(DEFAULT_TOKEN_THRESHOLD, 100_000);
    assert!(DEFAULT_SUMMARY_PROMPT.contains("<summary></summary>"));

    let enabled = CompactionControl::enabled();
    assert!(enabled.enabled);
    assert_eq!(
        enabled.context_token_threshold,
        Some(DEFAULT_TOKEN_THRESHOLD)
    );
    assert_eq!(
        enabled.summary_prompt.as_deref(),
        Some(DEFAULT_SUMMARY_PROMPT)
    );
}

#[test]
fn compaction_control_serializes_ts_field_names() {
    let control = CompactionControl {
        enabled: true,
        context_token_threshold: Some(123),
        model: Some("claude-opus-4-6".to_owned()),
        summary_prompt: Some("Summarize".to_owned()),
    };

    let json = serde_json::to_value(control).unwrap();
    assert_eq!(json["enabled"], true);
    assert_eq!(json["contextTokenThreshold"], 123);
    assert_eq!(json["model"], "claude-opus-4-6");
    assert_eq!(json["summaryPrompt"], "Summarize");
}

#[test]
fn compaction_control_default_is_disabled() {
    let control = CompactionControl::default();
    assert!(!control.enabled);
    assert!(control.context_token_threshold.is_none());
    assert!(control.model.is_none());
    assert!(control.summary_prompt.is_none());
}

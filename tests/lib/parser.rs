// Mirrors TS SDK tests/lib/parser.test.ts with Rust serde parser equivalents.

use anthropic_sdk::resources::beta::messages::{BetaContentBlock, BetaMessage};
use anthropic_sdk::resources::messages::{ContentBlock, Message};
use anthropic_sdk::sdk_lib::beta_parser::{
    maybe_parse_beta_message, parse_beta_message, parsed_beta_message_without_parsing,
    ParsedBetaContentBlock,
};
use anthropic_sdk::sdk_lib::parser::{
    maybe_parse_message, parse_message, parsed_message_without_parsing, ParsedContentBlock,
};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Weather {
    city: String,
    temperature: Option<i64>,
}

fn stable_message(content: Vec<ContentBlock>) -> Message {
    Message {
        id: "msg_123".to_owned(),
        content,
        model: "claude-3-5-sonnet-20241022".to_owned(),
        ..Default::default()
    }
}

fn beta_message(content: Vec<BetaContentBlock>) -> BetaMessage {
    BetaMessage {
        id: "msg_123".to_owned(),
        content,
        model: "claude-3-5-sonnet-20241022".to_owned(),
        ..Default::default()
    }
}

#[test]
fn stable_parse_message_parses_structured_output_and_block_level_output() {
    let message = stable_message(vec![ContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco","temperature":72}"#.to_owned(),
    }]);

    let parsed = parse_message::<Weather>(&message).unwrap();
    assert_eq!(
        parsed.parsed_output,
        Some(Weather {
            city: "San Francisco".to_owned(),
            temperature: Some(72),
        })
    );
    match &parsed.content[0] {
        ParsedContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().city, "San Francisco")
        }
        other => panic!("expected parsed text block, got {other:?}"),
    }
}

#[test]
fn stable_parse_message_validates_against_serde_type() {
    #[allow(dead_code)]
    #[derive(Debug, Deserialize)]
    struct StrictWeather {
        city: String,
        temperature: i64,
    }

    let message = stable_message(vec![ContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco","temperature":"not a number"}"#.to_owned(),
    }]);

    let err = parse_message::<StrictWeather>(&message).unwrap_err();
    assert!(
        err.to_string()
            .contains("Failed to parse structured output"),
        "unexpected error: {err}"
    );
}

#[test]
fn stable_parse_message_handles_invalid_json() {
    let message = stable_message(vec![ContentBlock::Text {
        citations: None,
        text: "invalid json".to_owned(),
    }]);

    let err = parse_message::<Weather>(&message).unwrap_err();
    assert!(
        err.to_string()
            .contains("Failed to parse structured output"),
        "unexpected error: {err}"
    );
}

#[test]
fn stable_parse_message_handles_messages_without_text_blocks() {
    let message = stable_message(vec![ContentBlock::ToolUse {
        id: "toolu_1".to_owned(),
        input: serde_json::json!({}),
        name: "get_weather".to_owned(),
    }]);

    let parsed = parse_message::<Weather>(&message).unwrap();
    assert!(parsed.parsed_output.is_none());
    assert_eq!(parsed.content.len(), 1);
    assert!(matches!(parsed.content[0], ParsedContentBlock::Other(_)));
}

#[test]
fn stable_parse_message_handles_multiple_text_blocks_like_ts() {
    let message = stable_message(vec![
        ContentBlock::Text {
            citations: None,
            text: r#"{"city":"San Francisco"}"#.to_owned(),
        },
        ContentBlock::Text {
            citations: None,
            text: r#"{"city":"Los Angeles"}"#.to_owned(),
        },
    ]);

    let parsed = parse_message::<Weather>(&message).unwrap();
    assert_eq!(parsed.parsed_output.as_ref().unwrap().city, "San Francisco");
    match &parsed.content[0] {
        ParsedContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().city, "San Francisco")
        }
        other => panic!("expected first parsed text block, got {other:?}"),
    }
    match &parsed.content[1] {
        ParsedContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().city, "Los Angeles")
        }
        other => panic!("expected second parsed text block, got {other:?}"),
    }
}

#[test]
fn stable_maybe_parse_and_no_parse_paths_match_ts_null_parsed_output_cases() {
    let message = stable_message(vec![ContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco"}"#.to_owned(),
    }]);

    let parsed = maybe_parse_message(&message);
    assert_eq!(parsed.parsed_output.unwrap()["city"], "San Francisco");

    let not_parseable = parsed_message_without_parsing::<serde_json::Value>(&message);
    assert!(not_parseable.parsed_output.is_none());
    match &not_parseable.content[0] {
        ParsedContentBlock::Text { parsed_output, .. } => assert!(parsed_output.is_none()),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn beta_parse_message_parses_structured_output_and_block_level_output() {
    let message = beta_message(vec![BetaContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco","temperature":72}"#.to_owned(),
    }]);

    let parsed = parse_beta_message::<Weather>(&message).unwrap();
    assert_eq!(parsed.parsed_output.as_ref().unwrap().city, "San Francisco");
    match &parsed.content[0] {
        ParsedBetaContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().temperature, Some(72))
        }
        other => panic!("expected parsed beta text block, got {other:?}"),
    }
}

#[test]
fn beta_parse_message_validates_and_reports_invalid_json() {
    let wrong_type = beta_message(vec![BetaContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco","temperature":"not a number"}"#.to_owned(),
    }]);
    assert!(parse_beta_message::<Weather>(&wrong_type).is_err());

    let invalid_json = beta_message(vec![BetaContentBlock::Text {
        citations: None,
        text: "invalid json".to_owned(),
    }]);
    let err = parse_beta_message::<Weather>(&invalid_json).unwrap_err();
    assert!(
        err.to_string()
            .contains("Failed to parse structured output"),
        "unexpected error: {err}"
    );
}

#[test]
fn beta_parse_message_handles_messages_without_text_blocks() {
    let message = beta_message(vec![BetaContentBlock::ToolUse {
        id: "toolu_1".to_owned(),
        input: serde_json::json!({}),
        name: "get_weather".to_owned(),
        caller: None,
    }]);

    let parsed = parse_beta_message::<Weather>(&message).unwrap();
    assert!(parsed.parsed_output.is_none());
    assert!(matches!(
        parsed.content[0],
        ParsedBetaContentBlock::Other(_)
    ));
}

#[test]
fn beta_parse_message_handles_multiple_text_blocks_like_ts() {
    let message = beta_message(vec![
        BetaContentBlock::Text {
            citations: None,
            text: r#"{"city":"San Francisco"}"#.to_owned(),
        },
        BetaContentBlock::Text {
            citations: None,
            text: r#"{"city":"Los Angeles"}"#.to_owned(),
        },
    ]);

    let parsed = parse_beta_message::<Weather>(&message).unwrap();
    assert_eq!(parsed.parsed_output.as_ref().unwrap().city, "San Francisco");
    match &parsed.content[1] {
        ParsedBetaContentBlock::Text { parsed_output, .. } => {
            assert_eq!(parsed_output.as_ref().unwrap().city, "Los Angeles")
        }
        other => panic!("expected second parsed beta text block, got {other:?}"),
    }
}

#[test]
fn beta_maybe_parse_and_no_parse_paths_match_ts_null_parsed_output_cases() {
    let message = beta_message(vec![BetaContentBlock::Text {
        citations: None,
        text: r#"{"city":"San Francisco"}"#.to_owned(),
    }]);

    let parsed = maybe_parse_beta_message(&message);
    assert_eq!(parsed.parsed_output.unwrap()["city"], "San Francisco");

    let not_parseable = parsed_beta_message_without_parsing::<serde_json::Value>(&message);
    assert!(not_parseable.parsed_output.is_none());
    match &not_parseable.content[0] {
        ParsedBetaContentBlock::Text { parsed_output, .. } => assert!(parsed_output.is_none()),
        other => panic!("expected beta text block, got {other:?}"),
    }
}

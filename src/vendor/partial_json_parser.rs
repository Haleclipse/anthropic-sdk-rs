// Maps to: TS _vendor/partial-json-parser/parser.ts

/// Token types produced by the partial-JSON lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenType {
    Brace,
    Paren,
    Separator,
    Delimiter,
    StringLit,
    Number,
    Name,
}

/// A single lexer token.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    token_type: TokenType,
    value: String,
}

/// Lexer: produces tokens from a (possibly incomplete) JSON string.
///
/// Handles `{ } [ ] : , "strings" numbers true/false/null`.
/// Escape sequences inside strings are preserved verbatim.
/// Unterminated strings (dangling quotes) are silently dropped.
fn tokenize(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut current: usize = 0;
    let mut tokens: Vec<Token> = Vec::new();

    while current < len {
        let ch = chars[current];

        // Top-level backslash (outside a string) -- skip like TS does.
        if ch == '\\' {
            current += 1;
            continue;
        }

        if ch == '{' {
            tokens.push(Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            });
            current += 1;
            continue;
        }

        if ch == '}' {
            tokens.push(Token {
                token_type: TokenType::Brace,
                value: "}".into(),
            });
            current += 1;
            continue;
        }

        if ch == '[' {
            tokens.push(Token {
                token_type: TokenType::Paren,
                value: "[".into(),
            });
            current += 1;
            continue;
        }

        if ch == ']' {
            tokens.push(Token {
                token_type: TokenType::Paren,
                value: "]".into(),
            });
            current += 1;
            continue;
        }

        if ch == ':' {
            tokens.push(Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            });
            current += 1;
            continue;
        }

        if ch == ',' {
            tokens.push(Token {
                token_type: TokenType::Delimiter,
                value: ",".into(),
            });
            current += 1;
            continue;
        }

        // String literal
        if ch == '"' {
            let mut value = String::new();
            let mut dangling_quote = false;

            current += 1; // skip opening quote

            while current < len && chars[current] != '"' {
                if chars[current] == '\\' {
                    // Preserve the backslash + next char as-is
                    let backslash = chars[current];
                    current += 1;
                    if current >= len {
                        dangling_quote = true;
                        break;
                    }
                    value.push(backslash);
                    value.push(chars[current]);
                    current += 1;
                } else {
                    value.push(chars[current]);
                    current += 1;
                }
            }

            if current >= len {
                dangling_quote = true;
            }

            // Skip closing quote (mirrors `char = input[++current]` in TS)
            current += 1;

            if !dangling_quote {
                tokens.push(Token {
                    token_type: TokenType::StringLit,
                    value,
                });
            }
            continue;
        }

        // Whitespace
        if ch.is_whitespace() {
            current += 1;
            continue;
        }

        // Numbers (digits, leading '-', leading '.')
        if ch.is_ascii_digit() || ch == '-' || ch == '.' {
            let mut value = String::new();

            if ch == '-' {
                value.push(ch);
                current += 1;
                if current >= len {
                    tokens.push(Token {
                        token_type: TokenType::Number,
                        value,
                    });
                    continue;
                }
            }

            while current < len && (chars[current].is_ascii_digit() || chars[current] == '.') {
                value.push(chars[current]);
                current += 1;
            }

            tokens.push(Token {
                token_type: TokenType::Number,
                value,
            });
            continue;
        }

        // Name tokens: true / false / null
        if ch.is_ascii_alphabetic() {
            let mut value = String::new();

            while current < len && chars[current].is_ascii_alphabetic() {
                value.push(chars[current]);
                current += 1;
            }

            if value == "true" || value == "false" || value == "null" {
                tokens.push(Token {
                    token_type: TokenType::Name,
                    value,
                });
            } else {
                // Unknown token (e.g. incomplete "nul") -- skip like TS does.
                current += 1;
            }
            continue;
        }

        // Anything else -- skip.
        current += 1;
    }

    tokens
}

/// Strips trailing incomplete tokens so the remaining sequence forms valid
/// (but possibly unclosed) JSON.
///
/// Rules applied recursively:
/// - Trailing Separator `:` -- remove and recurse.
/// - Trailing Delimiter `,` -- remove and recurse.
/// - Trailing Number ending in `.` or `-` -- remove and recurse.
/// - Trailing String or Number preceded by Delimiter -- remove both and recurse.
/// - Trailing String preceded by opening Brace `{` -- remove and recurse.
fn strip(mut tokens: Vec<Token>) -> Vec<Token> {
    if tokens.is_empty() {
        return tokens;
    }

    let last = &tokens[tokens.len() - 1];

    match last.token_type {
        TokenType::Separator => {
            tokens.pop();
            return strip(tokens);
        }
        TokenType::Delimiter => {
            tokens.pop();
            return strip(tokens);
        }
        TokenType::Number => {
            let last_char = last.value.chars().last();
            if last_char == Some('.') || last_char == Some('-') {
                tokens.pop();
                return strip(tokens);
            }
            // TS: fall-through from `case 'number'` into `case 'string'`
            let prev = tokens.get(tokens.len().wrapping_sub(2));
            if let Some(prev_token) = prev {
                if prev_token.token_type == TokenType::Delimiter {
                    tokens.pop();
                    return strip(tokens);
                }
                if prev_token.token_type == TokenType::Brace && prev_token.value == "{" {
                    tokens.pop();
                    return strip(tokens);
                }
            }
        }
        TokenType::StringLit => {
            let prev = tokens.get(tokens.len().wrapping_sub(2));
            if let Some(prev_token) = prev {
                if prev_token.token_type == TokenType::Delimiter {
                    tokens.pop();
                    return strip(tokens);
                }
                if prev_token.token_type == TokenType::Brace && prev_token.value == "{" {
                    tokens.pop();
                    return strip(tokens);
                }
            }
        }
        _ => {}
    }

    tokens
}

/// Auto-closes unclosed `{` and `[` by appending matching `}` / `]` tokens.
fn unstrip(mut tokens: Vec<Token>) -> Vec<Token> {
    let mut tail: Vec<char> = Vec::new();

    for token in &tokens {
        match token.token_type {
            TokenType::Brace => {
                if token.value == "{" {
                    tail.push('}');
                } else if token.value == "}" {
                    // Remove the last '}' from tail (mirrors splice(lastIndexOf, 1))
                    if let Some(pos) = tail.iter().rposition(|c| *c == '}') {
                        tail.remove(pos);
                    }
                }
            }
            TokenType::Paren => {
                if token.value == "[" {
                    tail.push(']');
                } else if token.value == "]" {
                    if let Some(pos) = tail.iter().rposition(|c| *c == ']') {
                        tail.remove(pos);
                    }
                }
            }
            _ => {}
        }
    }

    // Append closing tokens in reverse order
    for &closer in tail.iter().rev() {
        if closer == '}' {
            tokens.push(Token {
                token_type: TokenType::Brace,
                value: "}".into(),
            });
        } else if closer == ']' {
            tokens.push(Token {
                token_type: TokenType::Paren,
                value: "]".into(),
            });
        }
    }

    tokens
}

/// Reconstructs a JSON string from the token list.
/// `StringLit` values are wrapped in double-quotes; everything else is emitted
/// verbatim.
fn generate(tokens: &[Token]) -> String {
    let mut output = String::new();

    for token in tokens {
        match token.token_type {
            TokenType::StringLit => {
                output.push('"');
                output.push_str(&token.value);
                output.push('"');
            }
            _ => {
                output.push_str(&token.value);
            }
        }
    }

    output
}

/// Best-effort parser for incomplete JSON strings.
///
/// Pipeline: `tokenize` -> `strip` -> `unstrip` -> `generate` -> `serde_json::from_str`
///
/// Used during streaming to parse partial tool-use `input` payloads before the
/// server has finished emitting the full JSON object.
pub fn partial_parse(input: &str) -> Result<serde_json::Value, serde_json::Error> {
    let tokens = tokenize(input);
    let tokens = strip(tokens);
    let tokens = unstrip(tokens);
    let json_str = generate(&tokens);
    serde_json::from_str(&json_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- tokenize ---

    #[test]
    fn tokenize_empty_object() {
        let tokens = tokenize("{}");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].token_type, TokenType::Brace);
        assert_eq!(tokens[0].value, "{");
        assert_eq!(tokens[1].value, "}");
    }

    #[test]
    fn tokenize_string_with_escape() {
        let tokens = tokenize(r#""hello \"world""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::StringLit);
        assert_eq!(tokens[0].value, r#"hello \"world"#);
    }

    #[test]
    fn tokenize_dangling_quote_dropped() {
        let tokens = tokenize(r#"{"key": "incom"#);
        // The unterminated string "incom is dropped
        assert!(tokens
            .iter()
            .all(|t| !(t.token_type == TokenType::StringLit && t.value == "incom")));
    }

    #[test]
    fn tokenize_number_negative() {
        let tokens = tokenize("-42");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].token_type, TokenType::Number);
        assert_eq!(tokens[0].value, "-42");
    }

    #[test]
    fn tokenize_name_true_false_null() {
        let tokens = tokenize("true false null");
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|t| t.token_type == TokenType::Name));
    }

    #[test]
    fn tokenize_incomplete_name_dropped() {
        let tokens = tokenize("nul");
        assert!(tokens.is_empty());
    }

    // --- strip ---

    #[test]
    fn strip_trailing_separator() {
        let tokens = vec![
            Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            },
            Token {
                token_type: TokenType::StringLit,
                value: "key".into(),
            },
            Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            },
        ];
        let stripped = strip(tokens);
        // separator and the orphan key string should be gone
        assert_eq!(stripped.len(), 1);
        assert_eq!(stripped[0].value, "{");
    }

    #[test]
    fn strip_trailing_delimiter() {
        let tokens = vec![
            Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            },
            Token {
                token_type: TokenType::StringLit,
                value: "a".into(),
            },
            Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            },
            Token {
                token_type: TokenType::Number,
                value: "1".into(),
            },
            Token {
                token_type: TokenType::Delimiter,
                value: ",".into(),
            },
        ];
        let stripped = strip(tokens);
        assert_eq!(stripped.len(), 4);
        assert_eq!(stripped.last().map(|t| t.value.as_str()), Some("1"));
    }

    #[test]
    fn strip_trailing_number_with_dot() {
        let tokens = vec![Token {
            token_type: TokenType::Number,
            value: "3.".into(),
        }];
        let stripped = strip(tokens);
        assert!(stripped.is_empty());
    }

    // --- unstrip ---

    #[test]
    fn unstrip_closes_brace() {
        let tokens = vec![
            Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            },
            Token {
                token_type: TokenType::StringLit,
                value: "a".into(),
            },
            Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            },
            Token {
                token_type: TokenType::Number,
                value: "1".into(),
            },
        ];
        let result = unstrip(tokens);
        assert_eq!(result.last().map(|t| t.value.as_str()), Some("}"));
    }

    #[test]
    fn unstrip_closes_nested() {
        let tokens = vec![
            Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            },
            Token {
                token_type: TokenType::StringLit,
                value: "a".into(),
            },
            Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            },
            Token {
                token_type: TokenType::Paren,
                value: "[".into(),
            },
            Token {
                token_type: TokenType::Number,
                value: "1".into(),
            },
        ];
        let result = unstrip(tokens);
        let n = result.len();
        assert_eq!(result[n - 2].value, "]");
        assert_eq!(result[n - 1].value, "}");
    }

    // --- generate ---

    #[test]
    fn generate_wraps_strings() {
        let tokens = vec![
            Token {
                token_type: TokenType::Brace,
                value: "{".into(),
            },
            Token {
                token_type: TokenType::StringLit,
                value: "k".into(),
            },
            Token {
                token_type: TokenType::Separator,
                value: ":".into(),
            },
            Token {
                token_type: TokenType::Number,
                value: "1".into(),
            },
            Token {
                token_type: TokenType::Brace,
                value: "}".into(),
            },
        ];
        assert_eq!(generate(&tokens), r#"{"k":1}"#);
    }

    // --- partial_parse (end-to-end) ---

    #[test]
    fn partial_parse_complete_json() {
        let v = partial_parse(r#"{"a": 1, "b": "hello"}"#).unwrap();
        assert_eq!(v, json!({"a": 1, "b": "hello"}));
    }

    #[test]
    fn partial_parse_truncated_object() {
        let v = partial_parse(r#"{"a": 1, "b""#).unwrap();
        assert_eq!(v, json!({"a": 1}));
    }

    #[test]
    fn partial_parse_truncated_array() {
        // Trailing comma-preceded values are stripped recursively, leaving only
        // the first element (matches TS behaviour).
        let v = partial_parse(r#"[1, 2, 3"#).unwrap();
        assert_eq!(v, json!([1]));
    }

    #[test]
    fn partial_parse_nested_incomplete() {
        let v = partial_parse(r#"{"a": {"b": [1, 2"#).unwrap();
        assert_eq!(v, json!({"a": {"b": [1]}}));
    }

    #[test]
    fn partial_parse_complete_array() {
        // A fully closed array is preserved as-is.
        let v = partial_parse(r#"[1, 2, 3]"#).unwrap();
        assert_eq!(v, json!([1, 2, 3]));
    }

    #[test]
    fn partial_parse_trailing_comma() {
        let v = partial_parse(r#"{"a": 1,"#).unwrap();
        assert_eq!(v, json!({"a": 1}));
    }

    #[test]
    fn partial_parse_empty_object() {
        let v = partial_parse("{}").unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn partial_parse_booleans_and_null() {
        let v = partial_parse(r#"{"a": true, "b": false, "c": null}"#).unwrap();
        assert_eq!(v, json!({"a": true, "b": false, "c": null}));
    }

    #[test]
    fn partial_parse_dangling_string_value() {
        // Value string is unterminated -- gets dropped, then separator + key are stripped
        let v = partial_parse(r#"{"key": "incomp"#).unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn partial_parse_only_open_brace() {
        let v = partial_parse("{").unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn partial_parse_negative_number() {
        let v = partial_parse(r#"{"x": -5}"#).unwrap();
        assert_eq!(v, json!({"x": -5}));
    }

    #[test]
    fn partial_parse_decimal_number() {
        let v = partial_parse(r#"{"x": 2.72}"#).unwrap();
        assert_eq!(v, json!({"x": 2.72}));
    }

    #[test]
    fn partial_parse_incomplete_number_trailing_dot() {
        // "3." is not valid JSON -- strip should remove it
        let v = partial_parse(r#"{"x": 3."#).unwrap();
        assert_eq!(v, json!({}));
    }

    #[test]
    fn partial_parse_empty_input() {
        // Empty input produces no tokens -> empty generate -> serde error
        assert!(partial_parse("").is_err());
    }

    // -- TS-parity tests: a valid complete JSON string with all-string values --

    #[test]
    fn partial_parse_complete_string_values() {
        let v = partial_parse(r#"{"foo": "bar", "thing": "baz"}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar", "thing": "baz"}));
    }

    // -- TS-parity: a valid partial JSON string (dangling quote on value, multi-key) --

    #[test]
    fn partial_parse_dangling_quote_multi_key() {
        // {"foo": "bar", "thing": " -- dangling quote drops the value,
        // then strip removes the orphaned key. Earlier complete pairs survive.
        let v = partial_parse(r#"{"foo": "bar", "thing": ""#).unwrap();
        assert_eq!(v, json!({"foo": "bar"}));
    }

    // -- TS-parity: incomplete nested JSON object --

    #[test]
    fn partial_parse_incomplete_nested_object() {
        // Outer object missing closing brace, inner object complete.
        let v = partial_parse(r#"{"foo": {"bar": "baz"}"#).unwrap();
        assert_eq!(v, json!({"foo": {"bar": "baz"}}));
    }

    // -- TS-parity: complete nested JSON object --

    #[test]
    fn partial_parse_complete_nested_object() {
        let v = partial_parse(r#"{"foo": {"bar": "baz"}}"#).unwrap();
        assert_eq!(v, json!({"foo": {"bar": "baz"}}));
    }

    // -- TS-parity: JSON array with incomplete object --

    #[test]
    fn partial_parse_array_with_incomplete_object() {
        let v = partial_parse(r#"{"foo": [{"bar": "baz"}"#).unwrap();
        assert_eq!(v, json!({"foo": [{"bar": "baz"}]}));
    }

    // -- TS-parity: JSON array with complete objects --

    #[test]
    fn partial_parse_array_with_complete_objects() {
        let v = partial_parse(r#"{"foo": [{"bar": "baz"}, {"qux": "quux"}]}"#).unwrap();
        assert_eq!(v, json!({"foo": [{"bar": "baz"}, {"qux": "quux"}]}));
    }

    // -- TS-parity: string with escaped characters --

    #[test]
    fn partial_parse_escaped_characters() {
        let v = partial_parse(r#"{"foo": "bar\"baz"}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar\"baz"}));
    }

    // -- TS-parity: string with incomplete escape sequence --

    #[test]
    fn partial_parse_incomplete_escape_sequence() {
        // {"foo": "bar\ -- dangling quote due to incomplete escape
        let v = partial_parse(r#"{"foo": "bar\"#).unwrap();
        assert_eq!(v, json!({}));
    }

    // -- TS-parity: invalid JSON string gracefully (unclosed object, multi-key) --

    #[test]
    fn partial_parse_unclosed_object_multi_key() {
        let v = partial_parse(r#"{"foo": "bar", "thing": "baz""#).unwrap();
        assert_eq!(v, json!({"foo": "bar", "thing": "baz"}));
    }

    // -- TS-parity: JSON string with null value --

    #[test]
    fn partial_parse_null_with_string() {
        let v = partial_parse(r#"{"foo": null, "bar": "baz"}"#).unwrap();
        assert_eq!(v, json!({"foo": null, "bar": "baz"}));
    }

    // -- TS-parity: JSON string with number values --

    #[test]
    fn partial_parse_number_values() {
        let v = partial_parse(r#"{"foo": 123, "bar": 45.67}"#).unwrap();
        assert_eq!(v, json!({"foo": 123, "bar": 45.67}));
    }

    // -- TS-parity: JSON string with boolean values --

    #[test]
    fn partial_parse_boolean_values() {
        let v = partial_parse(r#"{"foo": true, "bar": false}"#).unwrap();
        assert_eq!(v, json!({"foo": true, "bar": false}));
    }

    // -- TS-parity: JSON string with mixed data types --

    #[test]
    fn partial_parse_mixed_data_types() {
        let v = partial_parse(r#"{"foo": "bar", "baz": 123, "qux": true, "quux": null}"#).unwrap();
        assert_eq!(
            v,
            json!({"foo": "bar", "baz": 123, "qux": true, "quux": null})
        );
    }

    // -- TS-parity: JSON string with partial literal tokens --

    #[test]
    fn partial_parse_partial_literal_nul() {
        let v = partial_parse(r#"{"foo": "bar", "baz": nul}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar"}));
    }

    #[test]
    fn partial_parse_partial_literal_tr() {
        let v = partial_parse(r#"{"foo": "bar", "baz": tr}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar"}));
    }

    #[test]
    fn partial_parse_partial_literal_truee() {
        // "truee" is not a valid name -- 'true' is consumed, then 'e' is
        // an extra character that gets skipped. But the trailing 'e' is
        // consumed by the name tokenizer (it reads while alphabetic), so
        // "truee" as a whole is not "true" and gets dropped.
        let v = partial_parse(r#"{"foo": "bar", "baz": truee}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar"}));
    }

    #[test]
    fn partial_parse_partial_literal_fal() {
        let v = partial_parse(r#"{"foo": "bar", "baz": fal}"#).unwrap();
        assert_eq!(v, json!({"foo": "bar"}));
    }

    // -- TS-parity: deeply nested JSON objects --

    #[test]
    fn partial_parse_deeply_nested_complete() {
        let v = partial_parse(r#"{"a": {"b": {"c": {"d": "e"}}}}"#).unwrap();
        assert_eq!(v, json!({"a": {"b": {"c": {"d": "e"}}}}));
    }

    #[test]
    fn partial_parse_deeply_nested_partial() {
        // Truncated mid-string-value: dangling quote drops "e",
        // then stripping removes "d":, and unstrip auto-closes all open braces.
        let v = partial_parse(r#"{"a": {"b": {"c": {"d": "e"#).unwrap();
        assert_eq!(v, json!({"a": {"b": {"c": {}}}}));
    }

    // -- TS-parity: string literals (standalone JSON string) --

    #[test]
    fn partial_parse_string_literal() {
        // A standalone JSON string value should parse correctly.
        let v = partial_parse(r#""hello world""#).unwrap();
        assert_eq!(v, json!("hello world"));
    }
}

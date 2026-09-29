// Maps to: TS internal/utils/env.ts
//
//! Environment-variable helpers.
//!
//! The TypeScript SDK's `readEnv()` is its only reader of the process
//! environment: it trims the value and returns `undefined` when the variable
//! is absent. [`read_env`] is likewise this SDK's only reader (`clippy.toml`
//! rejects `std::env::var*` elsewhere). Rust has no Deno/browser runtime, so
//! it reads the process environment directly.

use std::ffi::OsStr;

/// Read and trim an environment variable.
///
/// Maps to TS `readEnv(env)`: `process.env[env]?.trim() ?? undefined`. A set
/// but empty (or all-whitespace) variable is `Some("")`, not `None`: `??` only
/// replaces `undefined`.
#[allow(clippy::disallowed_methods)] // The SDK's single environment reader.
pub fn read_env(name: &str) -> Option<String> {
    std::env::var_os(name).map(|value| decode(&value))
}

/// Node decodes a non-UTF-8 value with U+FFFD replacements rather than
/// treating the variable as unset, then `readEnv` trims it.
fn decode(value: &OsStr) -> String {
    js_trim(&value.to_string_lossy()).to_owned()
}

/// `String.prototype.trim`: ECMAScript WhiteSpace and LineTerminator. That is
/// Unicode White_Space plus U+FEFF, minus U+0085, which `str::trim` strips.
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}')
}

/// TS-style camelCase alias for [`read_env`].
#[allow(non_snake_case)]
pub fn readEnv(name: &str) -> Option<String> {
    read_env(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_env_trims_and_returns_none_for_absent_values_like_ts() {
        if crate::child_env::run_in_child_env(
            module_path!(),
            "read_env_trims_and_returns_none_for_absent_values_like_ts",
            &[
                ("ANTHROPIC_SDK_RS_READ_ENV_PADDED", "  value with space  "),
                ("ANTHROPIC_SDK_RS_READ_ENV_BLANK", "   "),
            ],
        ) {
            return;
        }
        let absent = "ANTHROPIC_SDK_RS_READ_ENV_ABSENT";
        assert_eq!(read_env(absent), None);
        assert_eq!(readEnv(absent), None);

        let padded = "ANTHROPIC_SDK_RS_READ_ENV_PADDED";
        assert_eq!(read_env(padded), Some("value with space".to_owned()));
        assert_eq!(readEnv(padded), Some("value with space".to_owned()));

        assert_eq!(
            read_env("ANTHROPIC_SDK_RS_READ_ENV_BLANK"),
            Some(String::new())
        );
    }

    #[test]
    fn js_trim_uses_ecmascript_whitespace() {
        // JS strips the BOM, Rust's `trim` does not.
        assert_eq!(js_trim("\u{feff} key \u{feff}"), "key");
        // Rust's `trim` strips NEL, JS does not.
        assert_eq!(js_trim("\u{85}key\u{85}"), "\u{85}key\u{85}");
        assert_eq!(js_trim("\u{3000}\t\u{2028}key\r\n\u{a0}"), "key");
    }

    #[cfg(unix)]
    #[test]
    fn decode_replaces_invalid_utf8_like_node() {
        use std::os::unix::ffi::OsStrExt;
        // Node: `process.env.X` for bytes `a\xffb` is "a\u{fffd}b".
        let value = OsStr::from_bytes(b" a\xffb ");
        assert_eq!(decode(value), "a\u{fffd}b");
    }
}

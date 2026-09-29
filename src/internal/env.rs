// Maps to: TS internal/utils/env.ts
//
//! Environment-variable helpers.
//!
//! The TypeScript SDK's `readEnv()` trims leading/trailing whitespace and
//! returns `undefined` when an environment variable is absent or inaccessible.
//! Rust does not expose Deno/browser environments, so this helper maps to
//! `std::env::var`, trims the value, and returns `None` for missing or invalid
//! environment values.

/// Read and trim an environment variable.
///
/// Maps to TS `readEnv(env)`.
pub fn read_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
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
}

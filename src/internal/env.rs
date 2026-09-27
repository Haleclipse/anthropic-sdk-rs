// Maps to: TS internal/utils/env.ts
//
//! Environment-variable helpers.
//!
//! The TypeScript SDK's `readEnv()` trims leading/trailing whitespace and
//! returns `undefined` when an environment variable is absent or inaccessible.
//! Rust does not expose Deno/browser environments, so this helper maps to
//! `std::env::var`, trims the value, and returns `None` for missing or invalid
//! environment values.
//!
//! `readEnv()` reads the host's live `process.env`. A Rust host that keeps its
//! effective environment in-process — rather than mutating the real one, which
//! is unsound once other threads may read it — installs that reader once with
//! [`set_env_source`]; every SDK environment fallback then observes it.

use std::sync::OnceLock;

/// A host environment reader: the untrimmed value, or `None` when absent.
pub type EnvSource = fn(&str) -> Option<String>;

static ENV_SOURCE: OnceLock<EnvSource> = OnceLock::new();

/// Install the process-wide environment reader used by [`read_env`].
///
/// Only the first installation takes effect; later calls return their reader
/// back as `Err`. Without one, [`read_env`] reads the real environment.
pub fn set_env_source(source: EnvSource) -> Result<(), EnvSource> {
    ENV_SOURCE.set(source)
}

/// Read and trim an environment variable.
///
/// Maps to TS `readEnv(env)`.
pub fn read_env(name: &str) -> Option<String> {
    match ENV_SOURCE.get() {
        Some(source) => source(name),
        None => std::env::var(name).ok(),
    }
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
        let name = "ANTHROPIC_SDK_RS_READ_ENV_TEST";
        let original = std::env::var_os(name);

        std::env::remove_var(name);
        assert_eq!(read_env(name), None);
        assert_eq!(readEnv(name), None);

        std::env::set_var(name, "  value with space  ");
        assert_eq!(read_env(name), Some("value with space".to_owned()));
        assert_eq!(readEnv(name), Some("value with space".to_owned()));

        std::env::set_var(name, "   ");
        assert_eq!(read_env(name), Some(String::new()));

        match original {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

// Maps to: TS internal/utils.ts
//
//! Convenience re-export module for internal utility helpers.
//!
//! The TypeScript SDK's `src/internal/utils.ts` re-exports the individual
//! utility modules (`values`, `base64`, `env`, `log`, `uuid`, and `sleep`).
//! Rust keeps those modules available directly and mirrors the aggregate module
//! here for source-layout parity.

pub use super::base64::*;
pub use super::env::*;
pub use super::log::*;
pub use super::sleep::*;
pub use super::uuid::*;
pub use super::values::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utility_reexports_match_ts_internal_utils_barrel() {
        assert!(isAbsoluteURL("https://example.com"));
        assert_eq!(to_base64_str("hello"), "aGVsbG8=");
        assert_eq!(parseLogLevel(Some("debug")), Some("debug"));
        assert_eq!(uuid4().len(), 36);
    }
}

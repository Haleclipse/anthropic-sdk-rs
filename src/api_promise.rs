// Maps to: TS src/api-promise.ts
//
//! Deprecated top-level API promise re-export.
//!
//! Mirrors the TypeScript SDK's `src/api-promise.ts` compatibility barrel. In
//! Rust, import from [`crate::core::api_promise`] for the canonical path.

pub use crate::core::api_promise::*;

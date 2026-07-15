// Maps to: TS src/error.ts
//
//! Deprecated top-level error re-export.
//!
//! Mirrors the TypeScript SDK's `src/error.ts` compatibility barrel. In Rust,
//! import from [`crate::core::error`] for the canonical path.

pub use crate::core::error::*;

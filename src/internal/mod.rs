// Maps to: TS internal/
//
//! Internal utilities for the Anthropic Rust SDK.
//!
//! These modules provide low-level building blocks (line decoding, header
//! merging, request option types, response parsing helpers, error utilities,
//! and shared constants) used by the higher-level client and resource layers.

pub mod base64;
pub mod builtin_types;
pub mod bytes;
pub mod constants;
pub mod decoders;
pub mod detect_platform;
pub mod env;
pub mod errors;
pub mod headers;
pub mod log;
pub mod parse;
pub mod path;
pub mod query;
pub mod request_options;
pub mod shim_types;
pub mod shims;
pub mod sleep;
pub mod stream_utils;
pub mod to_file;
pub mod types;
pub mod uploads;
pub mod utils;
pub mod uuid;
pub mod values;

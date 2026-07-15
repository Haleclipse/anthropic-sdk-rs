// Maps to: TS lib/ directory
//
// High-level SDK helpers that sit above the core HTTP/SSE layer and the
// typed resource definitions. This mirrors the TypeScript SDK's `src/lib/`
// subtree, providing streaming accumulation, structured-output parsing,
// tool execution, and stainless helper-header utilities.

pub mod beta_message_stream;
pub mod beta_parser;
pub mod message_stream;
pub mod parser;
pub mod stainless_helper_header;
pub mod tools;
pub mod transform_json_schema;

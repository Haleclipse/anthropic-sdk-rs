// Maps to: TS helpers/
pub mod beta;
pub mod json_schema;
pub mod schemars;

// TS helpers/index.ts-style top-level helper exports.
pub use json_schema::{jsonSchemaOutputFormat, jsonSchemaOutputFormatWithOptions};

// Rust-native equivalents of TS helpers/zod.ts. We intentionally expose these
// under `schemars_*` names instead of `zod*` because Rust validation is based on
// `schemars::JsonSchema` + `serde` rather than JavaScript Zod values.
pub use schemars::{
    schemars_output_config, schemars_output_config_with_options, schemars_output_format,
    schemars_output_format_with_options, schemars_parse_output,
};

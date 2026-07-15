// Maps to: TS helpers/zod.ts, using Rust's `schemars` + `serde` stack.
//
//! Rust equivalent of the TypeScript SDK's Zod helper API.
//!
//! Public Rust APIs intentionally use `schemars` naming instead of `zod`:
//!
//! - `schemars::JsonSchema` generates JSON Schema from Rust types
//! - `serde::Deserialize` validates/parses model output at runtime
//! - `transform_json_schema` applies the same strict-schema transform as TS
//!
//! Unlike TS, the output-format object does not carry a JavaScript `parse`
//! closure. Parse responses with [`schemars_parse_output`] or
//! `sdk_lib::parser::parse_message::<T>()`.

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::core::error::ApiError;
use crate::helpers::json_schema::{
    json_schema_format_with_options, json_schema_output_format_with_options,
    JsonSchemaOutputFormatOptions,
};
use crate::resources::messages::{JsonOutputFormat, OutputConfig};

/// Rust equivalent of TS `zodOutputFormat(zodObject)`.
///
/// Generates a JSON Schema from Rust type `T`, transforms it using the same
/// strict-schema helper as TS, and returns a serializable JSON-schema output
/// format. Parse model output with [`schemars_parse_output::<T>`] or
/// `sdk_lib::parser::parse_message::<T>()`.
pub fn schemars_output_format<T>() -> Result<JsonOutputFormat, ApiError>
where
    T: JsonSchema,
{
    schemars_output_format_with_options::<T>(JsonSchemaOutputFormatOptions::default())
}

/// Same as [`schemars_output_format`] with explicit transform options.
pub fn schemars_output_format_with_options<T>(
    options: JsonSchemaOutputFormatOptions,
) -> Result<JsonOutputFormat, ApiError>
where
    T: JsonSchema,
{
    let schema = schema_for_type::<T>()?;
    json_schema_format_with_options(schema, options)
}

/// Convenience wrapper that returns an [`OutputConfig`] ready for
/// `MessageCreateParams.output_config`.
pub fn schemars_output_config<T>() -> Result<OutputConfig, ApiError>
where
    T: JsonSchema,
{
    schemars_output_config_with_options::<T>(JsonSchemaOutputFormatOptions::default())
}

/// Same as [`schemars_output_config`] with explicit transform options.
pub fn schemars_output_config_with_options<T>(
    options: JsonSchemaOutputFormatOptions,
) -> Result<OutputConfig, ApiError>
where
    T: JsonSchema,
{
    let schema = schema_for_type::<T>()?;
    json_schema_output_format_with_options(schema, options)
}

/// Parses structured-output text into `T` using serde.
///
/// This is the Rust counterpart to the parse closure returned by TS
/// `zodOutputFormat`. JSON syntax errors and serde validation errors are
/// reported as SDK errors with TS-like messages.
pub fn schemars_parse_output<T>(content: &str) -> Result<T, ApiError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(content).map_err(|err| {
        if err.is_syntax() || err.is_eof() {
            ApiError::Sdk(format!("Failed to parse structured output as JSON: {err}"))
        } else {
            ApiError::Sdk(format!("Failed to parse structured output: {err}"))
        }
    })
}

fn schema_for_type<T>() -> Result<serde_json::Value, ApiError>
where
    T: JsonSchema,
{
    serde_json::to_value(schemars::schema_for!(T))
        .map_err(|err| ApiError::Sdk(format!("failed to generate JSON schema: {err}")))
}

// Maps to: TS lib/transform-json-schema.ts
//
// Transforms arbitrary JSON Schema into the stricter subset accepted by
// Anthropic structured-output helpers. Unsupported validation keywords are
// preserved by appending them to the schema description, matching the TS SDK
// helper behavior.

use serde_json::{Map, Value};

use crate::core::error::ApiError;

/// Rust alias for TS `JSONSchema`.
pub type JSONSchema = Value;

const SUPPORTED_STRING_FORMATS: &[&str] = &[
    "date-time",
    "time",
    "date",
    "duration",
    "email",
    "hostname",
    "uri",
    "ipv4",
    "ipv6",
    "uuid",
];

/// Maps to TS `transformJSONSchema(jsonSchema)`.
///
/// The input value is consumed, so callers that need to keep the original schema
/// can clone it before calling. Internally this mirrors the TS helper's deep-copy
/// behavior by always constructing a fresh transformed schema.
pub fn transform_json_schema(json_schema: Value) -> Result<Value, ApiError> {
    transform_schema_value(json_schema)
}

/// TS-style camelCase alias for [`transform_json_schema`].
#[allow(non_snake_case)]
pub fn transformJSONSchema(json_schema: JSONSchema) -> Result<JSONSchema, ApiError> {
    transform_json_schema(json_schema)
}

fn transform_schema_value(value: Value) -> Result<Value, ApiError> {
    match value {
        Value::Object(map) => transform_schema_object(map),
        _ => Err(ApiError::Sdk(
            "JSON schema entries must be JSON objects".to_owned(),
        )),
    }
}

fn transform_schema_object(mut json_schema: Map<String, Value>) -> Result<Value, ApiError> {
    let mut strict_schema = Map::new();

    if let Some(reference) = json_schema.shift_remove("$ref") {
        strict_schema.insert("$ref".to_owned(), reference);
        return Ok(Value::Object(strict_schema));
    }

    if let Some(defs) = json_schema.shift_remove("$defs") {
        let mut strict_defs = Map::new();
        match defs {
            Value::Object(def_map) => {
                for (name, def_schema) in def_map {
                    strict_defs.insert(name, transform_schema_value(def_schema)?);
                }
            }
            other => {
                return Err(ApiError::Sdk(format!(
                    "JSON schema $defs must be an object, but got {}",
                    type_name(&other)
                )));
            }
        }
        strict_schema.insert("$defs".to_owned(), Value::Object(strict_defs));
    }

    let type_value = json_schema.shift_remove("type");
    let any_of = json_schema.shift_remove("anyOf");
    let one_of = json_schema.shift_remove("oneOf");
    let all_of = json_schema.shift_remove("allOf");

    if let Some(Value::Array(variants)) = any_of {
        strict_schema.insert("anyOf".to_owned(), transform_schema_array(variants)?);
    } else if let Some(Value::Array(variants)) = one_of {
        strict_schema.insert("anyOf".to_owned(), transform_schema_array(variants)?);
    } else if let Some(Value::Array(entries)) = all_of {
        strict_schema.insert("allOf".to_owned(), transform_schema_array(entries)?);
    } else if let Some(type_value) = type_value.clone() {
        strict_schema.insert("type".to_owned(), type_value);
    } else {
        return Err(ApiError::Sdk(
            "JSON schema must have a type defined if anyOf/oneOf/allOf are not used".to_owned(),
        ));
    }

    if let Some(description) = json_schema.shift_remove("description") {
        strict_schema.insert("description".to_owned(), description);
    }

    if let Some(title) = json_schema.shift_remove("title") {
        strict_schema.insert("title".to_owned(), title);
    }

    match type_value.as_ref().and_then(Value::as_str) {
        Some("object") => transform_object_schema(&mut json_schema, &mut strict_schema)?,
        Some("string") => transform_string_schema(&mut json_schema, &mut strict_schema),
        Some("array") => transform_array_schema(&mut json_schema, &mut strict_schema)?,
        _ => {}
    }

    if !json_schema.is_empty() {
        let leftovers = stringify_leftovers(&json_schema);
        let next_description = match strict_schema.get("description").cloned() {
            Some(Value::String(existing)) if !existing.is_empty() => {
                Value::String(format!("{existing}\n\n{leftovers}"))
            }
            Some(Value::String(_)) | None => Value::String(leftovers),
            Some(other) => Value::String(format!("{}\n\n{leftovers}", json_to_string(&other))),
        };
        strict_schema.insert("description".to_owned(), next_description);
    }

    Ok(Value::Object(strict_schema))
}

fn transform_schema_array(values: Vec<Value>) -> Result<Value, ApiError> {
    values
        .into_iter()
        .map(transform_schema_value)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn transform_object_schema(
    json_schema: &mut Map<String, Value>,
    strict_schema: &mut Map<String, Value>,
) -> Result<(), ApiError> {
    let properties = json_schema
        .shift_remove("properties")
        .unwrap_or_else(|| Value::Object(Map::new()));

    let mut strict_properties = Map::new();
    match properties {
        Value::Object(props) => {
            for (key, prop_schema) in props {
                strict_properties.insert(key, transform_schema_value(prop_schema)?);
            }
        }
        other => {
            return Err(ApiError::Sdk(format!(
                "JSON schema properties must be an object, but got {}",
                type_name(&other)
            )));
        }
    }

    strict_schema.insert("properties".to_owned(), Value::Object(strict_properties));

    // TS pops and discards any original additionalProperties value, then forces false.
    json_schema.shift_remove("additionalProperties");
    strict_schema.insert("additionalProperties".to_owned(), Value::Bool(false));

    if let Some(required) = json_schema.shift_remove("required") {
        strict_schema.insert("required".to_owned(), required);
    }

    Ok(())
}

fn transform_string_schema(
    json_schema: &mut Map<String, Value>,
    strict_schema: &mut Map<String, Value>,
) {
    if let Some(format) = json_schema.shift_remove("format") {
        if format
            .as_str()
            .is_some_and(|f| SUPPORTED_STRING_FORMATS.contains(&f))
        {
            strict_schema.insert("format".to_owned(), format);
        } else {
            // Unsupported formats are moved to description with other leftovers.
            json_schema.insert("format".to_owned(), format);
        }
    }
}

fn transform_array_schema(
    json_schema: &mut Map<String, Value>,
    strict_schema: &mut Map<String, Value>,
) -> Result<(), ApiError> {
    if let Some(items) = json_schema.shift_remove("items") {
        strict_schema.insert("items".to_owned(), transform_schema_value(items)?);
    }

    if let Some(min_items) = json_schema.shift_remove("minItems") {
        if matches!(min_items.as_i64(), Some(0 | 1)) {
            strict_schema.insert("minItems".to_owned(), min_items);
        } else {
            json_schema.insert("minItems".to_owned(), min_items);
        }
    }

    Ok(())
}

fn stringify_leftovers(leftovers: &Map<String, Value>) -> String {
    let entries = leftovers
        .iter()
        .map(|(key, value)| format!("{key}: {}", json_to_string(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{entries}}}")
}

fn json_to_string(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

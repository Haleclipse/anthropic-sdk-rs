// Ported from TS SDK: tests/helpers/transform-json-schema.test.ts

use anthropic_sdk::sdk_lib::transform_json_schema::{
    JSONSchema, transform_json_schema, transformJSONSchema,
};
use serde_json::json;

#[test]
fn transform_json_schema_ts_style_aliases_are_available() {
    let schema: JSONSchema = json!({"type": "object"});
    let transformed = transformJSONSchema(schema).unwrap();
    assert_eq!(transformed["additionalProperties"], false);
}

#[test]
fn transform_json_schema_does_not_mutate_original_when_cloned_by_caller() {
    let input = json!({
        "type": "object",
        "properties": {
            "bonus": {
                "type": "integer",
                "default": 100000,
                "minimum": 100000,
                "title": "Bonus",
                "description": "Annual bonus in USD"
            },
            "tags": {
                "type": "array",
                "items": {"type": "string"},
                "minItems": 3
            }
        },
        "title": "Employee",
        "additionalProperties": true
    });
    let input_copy = input.clone();

    let _ = transform_json_schema(input.clone()).unwrap();

    assert_eq!(input, input_copy);
}

#[test]
fn transform_json_schema_moves_unsupported_properties_to_description() {
    let input = json!({
        "type": "object",
        "properties": {
            "bonus": {
                "type": "integer",
                "default": 100000,
                "minimum": 100000,
                "title": "Bonus",
                "description": "Annual bonus in USD"
            }
        },
        "title": "Employee"
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["additionalProperties"], false);
    assert!(result["properties"]["bonus"].get("default").is_none());
    assert!(result["properties"]["bonus"].get("minimum").is_none());
    assert_eq!(result["properties"]["bonus"]["title"], "Bonus");
    assert_eq!(
        result["properties"]["bonus"]["description"],
        "Annual bonus in USD\n\n{default: 100000, minimum: 100000}"
    );
}

#[test]
fn transform_json_schema_handles_objects_without_existing_description() {
    let input = json!({
        "type": "object",
        "properties": {
            "count": {
                "type": "integer",
                "maximum": 10,
                "minimum": 1
            }
        }
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["additionalProperties"], false);
    assert_eq!(
        result["properties"]["count"]["description"],
        "{maximum: 10, minimum: 1}"
    );
    assert!(result["properties"]["count"].get("maximum").is_none());
    assert!(result["properties"]["count"].get("minimum").is_none());
}

#[test]
fn transform_json_schema_preserves_supported_min_items_values() {
    let input = json!({
        "type": "array",
        "items": {"type": "string"},
        "minItems": 1
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["minItems"], 1);
}

#[test]
fn transform_json_schema_moves_unsupported_min_items_values_to_description() {
    let input = json!({
        "type": "array",
        "items": {"type": "string"},
        "minItems": 3,
        "description": "List of items"
    });

    let result = transform_json_schema(input).unwrap();
    assert!(result.get("minItems").is_none());
    assert_eq!(result["description"], "List of items\n\n{minItems: 3}");
}

#[test]
fn transform_json_schema_handles_nested_objects_recursively() {
    let input = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "age": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 120
                    }
                }
            }
        }
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["additionalProperties"], false);
    assert_eq!(result["properties"]["user"]["additionalProperties"], false);
    assert_eq!(
        result["properties"]["user"]["properties"]["age"]["description"],
        "{minimum: 0, maximum: 120}"
    );
}

#[test]
fn transform_json_schema_handles_defs_and_refs_recursively() {
    let input = json!({
        "type": "object",
        "$defs": {
            "Person": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "pattern": "^[A-Za-z]+$"
                    }
                }
            }
        },
        "properties": {
            "person": {"$ref": "#/$defs/Person"}
        }
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["$defs"]["Person"]["additionalProperties"], false);
    assert_eq!(
        result["$defs"]["Person"]["properties"]["name"]["description"],
        "{pattern: \"^[A-Za-z]+$\"}"
    );
    assert_eq!(result["properties"]["person"]["$ref"], "#/$defs/Person");
}

#[test]
fn transform_json_schema_sets_additional_properties_false() {
    let input = json!({
        "type": "object",
        "properties": {"name": {"type": "string"}},
        "additionalProperties": true
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["additionalProperties"], false);
}

#[test]
fn transform_json_schema_preserves_supported_string_formats() {
    let input = json!({
        "type": "object",
        "properties": {
            "email": {"type": "string", "format": "email"},
            "date": {"type": "string", "format": "date-time"},
            "website": {"type": "string", "format": "uri"}
        }
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["properties"]["email"]["format"], "email");
    assert_eq!(result["properties"]["date"]["format"], "date-time");
    assert_eq!(result["properties"]["website"]["format"], "uri");
}

#[test]
fn transform_json_schema_moves_unsupported_string_formats_to_description() {
    let input = json!({
        "type": "object",
        "properties": {
            "password": {
                "type": "string",
                "format": "password",
                "description": "User password"
            },
            "customField": {
                "type": "string",
                "format": "custom-format"
            }
        }
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(
        result["properties"]["password"]["description"],
        "User password\n\n{format: \"password\"}"
    );
    assert_eq!(
        result["properties"]["customField"]["description"],
        "{format: \"custom-format\"}"
    );
    assert!(result["properties"]["password"].get("format").is_none());
    assert!(result["properties"]["customField"].get("format").is_none());
}

#[test]
fn transform_json_schema_transforms_all_of_recursively() {
    let input = json!({
        "allOf": [
            {
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "minimum": 1, "maximum": 999}
                }
            },
            {
                "type": "object",
                "properties": {
                    "name": {"type": "string", "pattern": "^[A-Z]", "minLength": 2}
                },
                "additionalProperties": true
            },
            {
                "type": "object",
                "properties": {
                    "tags": {"type": "array", "items": {"type": "string"}, "minItems": 5}
                }
            }
        ]
    });

    let result = transform_json_schema(input).unwrap();
    assert_eq!(result["allOf"][0]["additionalProperties"], false);
    assert_eq!(
        result["allOf"][0]["properties"]["id"]["description"],
        "{minimum: 1, maximum: 999}"
    );
    assert_eq!(result["allOf"][1]["additionalProperties"], false);
    assert_eq!(
        result["allOf"][1]["properties"]["name"]["description"],
        "{pattern: \"^[A-Z]\", minLength: 2}"
    );
    assert_eq!(result["allOf"][2]["additionalProperties"], false);
    assert_eq!(
        result["allOf"][2]["properties"]["tags"]["description"],
        "{minItems: 5}"
    );
}

#[test]
fn transform_json_schema_errors_when_type_missing_without_composition() {
    let err = transform_json_schema(json!({"properties": {"x": {"type": "string"}}}))
        .expect_err("schema without type should fail");
    assert!(
        err.to_string()
            .contains("JSON schema must have a type defined")
    );
}

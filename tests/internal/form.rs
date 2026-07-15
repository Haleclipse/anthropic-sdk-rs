// Mirrors TS SDK tests/form.test.ts for multipart form flattening.

use anthropic_sdk::core::uploads::{
    flatten_form_fields, form_null, form_undefined, FormField, FormValue, Uploadable,
};

fn text_entries(fields: &[FormField]) -> Vec<(&str, &str)> {
    fields
        .iter()
        .filter_map(|field| match field {
            FormField::Text { name, value } => Some((name.as_str(), value.as_str())),
            FormField::File { .. } => None,
        })
        .collect()
}

#[test]
fn multipart_form_accepts_valid_primitives_and_files() {
    let fields = flatten_form_fields(vec![
        ("foo", FormValue::from("foo")),
        ("string", FormValue::from(1_i64)),
        ("bool", FormValue::from(true)),
        (
            "file",
            FormValue::from(Uploadable::from_bytes(b"some-content".to_vec(), "file.txt")),
        ),
    ])
    .unwrap();

    assert_eq!(
        text_entries(&fields),
        vec![("foo", "foo"), ("string", "1"), ("bool", "true")]
    );
    assert!(matches!(fields.last(), Some(FormField::File { name, .. }) if name == "file"));
}

#[test]
fn multipart_form_rejects_null_like_ts() {
    let err = flatten_form_fields(vec![("null", form_null())]).unwrap_err();
    assert!(err
        .to_string()
        .contains("Received null for \"null\"; to pass null in FormData"));
}

#[test]
fn multipart_form_strips_undefined_recursively_like_ts() {
    let fields = flatten_form_fields(vec![
        ("foo", form_undefined()),
        ("bar", FormValue::from("baz")),
    ])
    .unwrap();
    assert_eq!(text_entries(&fields), vec![("bar", "baz")]);

    let fields = flatten_form_fields(vec![(
        "bar",
        FormValue::Object(vec![("baz".to_owned(), form_undefined())]),
    )])
    .unwrap();
    assert!(fields.is_empty());

    let fields = flatten_form_fields(vec![(
        "bar",
        FormValue::Object(vec![
            ("foo".to_owned(), FormValue::from("string")),
            ("baz".to_owned(), form_undefined()),
        ]),
    )])
    .unwrap();
    assert_eq!(text_entries(&fields), vec![("bar[foo]", "string")]);

    let fields = flatten_form_fields(vec![(
        "bar",
        FormValue::Array(vec![form_undefined(), form_undefined()]),
    )])
    .unwrap();
    assert!(fields.is_empty());

    let fields = flatten_form_fields(vec![(
        "bar",
        FormValue::Array(vec![form_undefined(), FormValue::from("foo")]),
    )])
    .unwrap();
    assert_eq!(text_entries(&fields), vec![("bar[]", "foo")]);
}

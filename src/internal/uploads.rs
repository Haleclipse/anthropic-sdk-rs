// Maps to: TS internal/uploads.ts
//
//! Internal multipart/form upload helpers.
//!
//! Rust's implementation lives in [`crate::core::uploads`] so resource methods
//! and callers share one multipart pipeline. This module mirrors the TS
//! internal namespace by re-exporting the form-flattening primitives used by
//! `multipartFormRequestOptions()`.

pub use crate::core::uploads::{
    FormField, FormValue, ToFileInput, Uploadable, create_form, flatten_form_fields,
    form_from_fields, form_null, form_undefined,
};

/// Rust equivalent of TS `checkFileSupport()`.
///
/// The JavaScript helper verifies that a global `File` constructor exists.
/// Rust uses `reqwest::multipart::Part` directly, so file uploads are available
/// whenever the crate is compiled.
pub fn check_file_support() -> Result<(), crate::ApiError> {
    Ok(())
}

/// TS-style camelCase alias for [`check_file_support`].
#[allow(non_snake_case)]
pub fn checkFileSupport() -> Result<(), crate::ApiError> {
    check_file_support()
}

/// Rust equivalent of TS `makeFile()` for in-memory file parts.
///
/// `file_name` defaults to `unknown_file`, matching the TS helper.
pub fn make_file(
    file_bits: impl Into<Vec<u8>>,
    file_name: Option<impl Into<String>>,
    mime_type: Option<impl Into<String>>,
) -> Uploadable {
    let filename = file_name
        .map(Into::into)
        .unwrap_or_else(|| "unknown_file".to_owned());
    match mime_type {
        Some(mime_type) => Uploadable::from_bytes_with_mime(file_bits, filename, mime_type),
        None => Uploadable::from_bytes(file_bits, filename),
    }
}

/// TS-style camelCase alias for [`make_file`].
#[allow(non_snake_case)]
pub fn makeFile(
    file_bits: impl Into<Vec<u8>>,
    file_name: Option<impl Into<String>>,
    mime_type: Option<impl Into<String>>,
) -> Uploadable {
    make_file(file_bits, file_name, mime_type)
}

/// Rust equivalent of TS `getName(value, stripPath)` for [`Uploadable`].
pub fn get_name(value: &Uploadable, strip_path: bool) -> String {
    value.filename(strip_path)
}

/// TS-style camelCase alias for [`get_name`].
#[allow(non_snake_case)]
pub fn getName(value: &Uploadable, strip_path: bool) -> String {
    get_name(value, strip_path)
}

/// Detect whether a Rust form value tree contains an uploadable file part.
pub fn has_uploadable_value(value: &FormValue) -> bool {
    match value {
        FormValue::Upload(_) => true,
        FormValue::Array(values) => values.iter().any(has_uploadable_value),
        FormValue::Object(entries) => entries.iter().any(|(_, value)| has_uploadable_value(value)),
        FormValue::String(_)
        | FormValue::Number(_)
        | FormValue::Bool(_)
        | FormValue::Null
        | FormValue::Undefined => false,
    }
}

/// TS-style camelCase alias for [`has_uploadable_value`].
#[allow(non_snake_case)]
pub fn hasUploadableValue(value: &FormValue) -> bool {
    has_uploadable_value(value)
}

/// Rust equivalent of TS `multipartFormRequestOptions()`.
///
/// The TS helper returns request options with a multipart body. Rust resource
/// methods call the client's `post_multipart*` helpers directly, so this
/// function exposes the form construction portion for parity and tests.
pub fn multipart_form_request_options<I, K>(
    entries: I,
) -> Result<reqwest::multipart::Form, crate::ApiError>
where
    I: IntoIterator<Item = (K, FormValue)>,
    K: Into<String>,
{
    create_form(entries, true)
}

/// TS-style camelCase alias for [`multipart_form_request_options`].
#[allow(non_snake_case)]
pub fn multipartFormRequestOptions<I, K>(
    entries: I,
) -> Result<reqwest::multipart::Form, crate::ApiError>
where
    I: IntoIterator<Item = (K, FormValue)>,
    K: Into<String>,
{
    multipart_form_request_options(entries)
}

/// TS-style `isAsyncIterable()` for Rust futures streams.
///
/// JavaScript detects `Symbol.asyncIterator` dynamically. Rust represents this
/// capability statically with [`futures::Stream`], so the helper is available
/// for values that implement that trait.
pub fn is_async_iterable<T>(_value: &T) -> bool
where
    T: futures::Stream + ?Sized,
{
    true
}

/// TS-style camelCase alias for [`is_async_iterable`].
#[allow(non_snake_case)]
pub fn isAsyncIterable<T>(value: &T) -> bool
where
    T: futures::Stream + ?Sized,
{
    is_async_iterable(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_file_support_and_make_file_aliases_match_rust_upload_model() {
        checkFileSupport().unwrap();

        let file = makeFile(b"hello".to_vec(), None::<String>, Some("text/plain"));
        assert_eq!(getName(&file, true), "unknown_file");
        assert_eq!(get_name(&file, false), "unknown_file");

        let named = make_file(b"{}".to_vec(), Some("/tmp/data.json"), None::<String>);
        assert_eq!(getName(&named, true), "data.json");
        assert_eq!(getName(&named, false), "/tmp/data.json");
    }

    #[test]
    fn has_uploadable_value_walks_nested_form_values_like_ts() {
        let value = FormValue::Object(vec![(
            "nested".to_owned(),
            FormValue::Array(vec![FormValue::from("text"), FormValue::Undefined]),
        )]);
        assert!(!has_uploadable_value(&value));

        let value = FormValue::Object(vec![(
            "nested".to_owned(),
            FormValue::Array(vec![FormValue::Upload(Uploadable::from_bytes(
                "hello",
                "hello.txt",
            ))]),
        )]);
        assert!(hasUploadableValue(&value));
    }

    #[test]
    fn multipart_form_request_options_alias_builds_form_from_core_upload_helpers() {
        let form = multipartFormRequestOptions(vec![
            ("name", FormValue::from("example")),
            (
                "file",
                FormValue::Upload(Uploadable::from_bytes("hello", "hello.txt")),
            ),
        ])
        .unwrap();
        let _ = form;
    }

    #[test]
    fn is_async_iterable_name_parity_helper_is_available_for_futures_streams() {
        let stream = futures::stream::iter([1, 2, 3]);
        assert!(is_async_iterable(&stream));
        assert!(isAsyncIterable(&stream));
    }
}

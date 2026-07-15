// Maps to: TS core/uploads.ts + internal/uploads.ts
//
//! Upload helpers for multipart/form-data endpoints.
//!
//! The TypeScript SDK accepts browser `File`/`Blob`, `Response`, and Node
//! streams. In Rust we expose an idiomatic [`Uploadable`] enum that can carry a
//! filesystem path or in-memory bytes and can be converted into reqwest
//! multipart parts.

use std::path::{Path, PathBuf};

use reqwest::multipart::Part;

use crate::core::error::ApiError;

/// Maps to: TS `Uploadable`.
///
/// Rust representation of a value that can be sent as a multipart file part.
/// Use [`Uploadable::from_path`] for filesystem uploads or
/// [`Uploadable::from_bytes`] / [`Uploadable::from_bytes_with_mime`] for
/// in-memory content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Uploadable {
    /// Upload a file from disk.
    Path(PathBuf),

    /// Upload in-memory bytes with an explicit filename and optional MIME type.
    Bytes {
        filename: String,
        bytes: Vec<u8>,
        mime_type: Option<String>,
    },

    /// Wrap another uploadable with SDK-helper metadata used to populate the
    /// `x-stainless-helper` header. This maps to TS helper-created files that
    /// carry `SDK_HELPER_SYMBOL`.
    WithHelper {
        upload: Box<Uploadable>,
        helper: String,
    },
}

/// Rust equivalent of TS `ToFileInput`.
///
/// TypeScript accepts `File`/`Blob`/`Response`/streams; Rust models the same
/// multipart-upload concept with [`Uploadable`]. Values such as `PathBuf`,
/// `&Path`, and `&str` convert into `Uploadable` via `Into`.
pub type ToFileInput = Uploadable;

/// Rust equivalent of TS `toFile()`.
///
/// In JS this normalizes many runtime upload shapes into a `File`. In Rust the
/// normalized upload shape is [`Uploadable`], so this helper is intentionally a
/// lightweight `Into<Uploadable>` conversion for API/import parity.
pub fn to_file(value: impl Into<Uploadable>) -> Uploadable {
    value.into()
}

impl Uploadable {
    /// Create an uploadable file from a filesystem path.
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self::Path(path.into())
    }

    /// Create an uploadable file from bytes and a filename.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>, filename: impl Into<String>) -> Self {
        Self::Bytes {
            filename: filename.into(),
            bytes: bytes.into(),
            mime_type: None,
        }
    }

    /// Create an uploadable file from bytes, filename, and MIME type.
    pub fn from_bytes_with_mime(
        bytes: impl Into<Vec<u8>>,
        filename: impl Into<String>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self::Bytes {
            filename: filename.into(),
            bytes: bytes.into(),
            mime_type: Some(mime_type.into()),
        }
    }

    /// Attach SDK-helper metadata to this uploadable.
    pub fn with_stainless_helper(self, helper: impl Into<String>) -> Self {
        Self::WithHelper {
            upload: Box::new(self),
            helper: helper.into(),
        }
    }

    /// Return SDK-helper metadata, if this uploadable was produced by a helper.
    pub fn stainless_helper(&self) -> Option<&str> {
        match self {
            Self::WithHelper { helper, .. } => Some(helper.as_str()),
            _ => None,
        }
    }

    /// Return the filename to put in the multipart content-disposition.
    ///
    /// Mirrors TS `getName(value, stripPath)`: when `strip_path` is true,
    /// filesystem paths and path-like filenames are reduced to their final
    /// component; otherwise the original path/name string is preserved.
    pub fn filename(&self, strip_path: bool) -> String {
        match self {
            Self::Path(path) => filename_from_path(path, strip_path),
            Self::Bytes { filename, .. } => {
                if strip_path {
                    Path::new(filename)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or(filename)
                        .to_owned()
                } else {
                    filename.clone()
                }
            }
            Self::WithHelper { upload, .. } => upload.filename(strip_path),
        }
    }

    /// Convert this uploadable into a reqwest multipart [`Part`].
    pub fn to_part(&self, strip_path: bool) -> Result<Part, ApiError> {
        let filename = self.filename(strip_path);
        match self {
            Self::Path(path) => {
                let bytes = std::fs::read(path).map_err(|e| {
                    ApiError::Sdk(format!(
                        "failed to read upload file '{}': {e}",
                        path.display()
                    ))
                })?;
                Ok(Part::bytes(bytes).file_name(filename))
            }
            Self::Bytes {
                bytes, mime_type, ..
            } => {
                let mut part = Part::bytes(bytes.clone()).file_name(filename);
                if let Some(mime) = mime_type {
                    part = part.mime_str(mime).map_err(|e| {
                        ApiError::Sdk(format!("invalid upload MIME type '{mime}': {e}"))
                    })?;
                }
                Ok(part)
            }
            Self::WithHelper { upload, .. } => upload.to_part(strip_path),
        }
    }
}

impl From<PathBuf> for Uploadable {
    fn from(value: PathBuf) -> Self {
        Self::from_path(value)
    }
}

impl From<&Path> for Uploadable {
    fn from(value: &Path) -> Self {
        Self::from_path(value)
    }
}

impl From<&str> for Uploadable {
    fn from(value: &str) -> Self {
        Self::from_path(value)
    }
}

fn filename_from_path(path: &Path, strip_path: bool) -> String {
    if strip_path {
        path.file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("unknown_file")
            .to_owned()
    } else {
        path.to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------------------
// TS-style multipart form flattening
// ---------------------------------------------------------------------------

/// Maps to TS internal/uploads.ts `addFormValue()` accepted values.
///
/// Rust callers can build nested multipart bodies explicitly with this enum.
/// `Undefined` values are stripped recursively, while `Null` produces the same
/// validation error as the TS SDK because multipart form data has no native
/// null representation.
#[derive(Debug, Clone, PartialEq)]
pub enum FormValue {
    String(String),
    Number(String),
    Bool(bool),
    Upload(Uploadable),
    Array(Vec<FormValue>),
    Object(Vec<(String, FormValue)>),
    Null,
    Undefined,
}

impl From<&str> for FormValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for FormValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<bool> for FormValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for FormValue {
    fn from(value: i64) -> Self {
        Self::Number(value.to_string())
    }
}

impl From<u64> for FormValue {
    fn from(value: u64) -> Self {
        Self::Number(value.to_string())
    }
}

impl From<f64> for FormValue {
    fn from(value: f64) -> Self {
        if value.fract() == 0.0 && value.is_finite() {
            Self::Number(format!("{value:.0}"))
        } else {
            Self::Number(value.to_string())
        }
    }
}

impl From<Uploadable> for FormValue {
    fn from(value: Uploadable) -> Self {
        Self::Upload(value)
    }
}

/// Sentinel for TS `null` in multipart form data.
pub fn form_null() -> FormValue {
    FormValue::Null
}

/// Sentinel for TS `undefined` in multipart form data.
pub fn form_undefined() -> FormValue {
    FormValue::Undefined
}

/// Text or file field produced by TS-style multipart flattening.
#[derive(Debug, Clone, PartialEq)]
pub enum FormField {
    Text { name: String, value: String },
    File { name: String, value: Uploadable },
}

impl FormField {
    pub fn name(&self) -> &str {
        match self {
            FormField::Text { name, .. } | FormField::File { name, .. } => name,
        }
    }
}

/// Maps to TS `createForm()` flattening, but returns inspectable fields.
///
/// Nested objects are encoded as `parent[child]`; arrays are encoded as
/// repeated `name[]` fields. `Undefined` values are skipped recursively and
/// `Null` returns an SDK error with the TS error text.
pub fn flatten_form_fields<I, K>(entries: I) -> Result<Vec<FormField>, ApiError>
where
    I: IntoIterator<Item = (K, FormValue)>,
    K: Into<String>,
{
    let mut fields = Vec::new();
    for (key, value) in entries {
        add_form_value(&mut fields, key.into(), value)?;
    }
    Ok(fields)
}

/// Build a `reqwest::multipart::Form` from TS-style form values.
///
/// `strip_filenames` maps to TS `multipartFormRequestOptions(...,
/// stripFilenames)`. Resource methods that need endpoint-specific behavior may
/// continue building forms manually, but this helper provides the general core
/// multipart behavior.
pub fn create_form<I, K>(
    entries: I,
    strip_filenames: bool,
) -> Result<reqwest::multipart::Form, ApiError>
where
    I: IntoIterator<Item = (K, FormValue)>,
    K: Into<String>,
{
    form_from_fields(flatten_form_fields(entries)?, strip_filenames)
}

/// Convert already-flattened fields into a `reqwest::multipart::Form`.
pub fn form_from_fields(
    fields: Vec<FormField>,
    strip_filenames: bool,
) -> Result<reqwest::multipart::Form, ApiError> {
    let mut form = reqwest::multipart::Form::new();
    for field in fields {
        match field {
            FormField::Text { name, value } => {
                form = form.text(name, value);
            }
            FormField::File { name, value } => {
                form = form.part(name, value.to_part(strip_filenames)?);
            }
        }
    }
    Ok(form)
}

fn add_form_value(
    fields: &mut Vec<FormField>,
    key: String,
    value: FormValue,
) -> Result<(), ApiError> {
    match value {
        FormValue::Undefined => Ok(()),
        FormValue::Null => Err(ApiError::Sdk(format!(
            "Received null for \"{key}\"; to pass null in FormData, you must use the string 'null'"
        ))),
        FormValue::String(value) | FormValue::Number(value) => {
            fields.push(FormField::Text { name: key, value });
            Ok(())
        }
        FormValue::Bool(value) => {
            fields.push(FormField::Text {
                name: key,
                value: value.to_string(),
            });
            Ok(())
        }
        FormValue::Upload(value) => {
            fields.push(FormField::File { name: key, value });
            Ok(())
        }
        FormValue::Array(values) => {
            for value in values {
                add_form_value(fields, format!("{key}[]"), value)?;
            }
            Ok(())
        }
        FormValue::Object(values) => {
            for (name, value) in values {
                add_form_value(fields, format!("{key}[{name}]"), value)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_strips_path_for_paths() {
        let upload = Uploadable::from_path("/tmp/example/data.json");
        assert_eq!(upload.filename(true), "data.json");
    }

    #[test]
    fn filename_preserves_path_when_requested() {
        let upload = Uploadable::from_path("/tmp/example/data.json");
        assert_eq!(upload.filename(false), "/tmp/example/data.json");
    }

    #[test]
    fn to_file_normalizes_into_uploadable_like_ts_to_file() {
        let upload = to_file("/tmp/example/data.json");
        assert_eq!(upload, Uploadable::from_path("/tmp/example/data.json"));

        let input = Uploadable::from_bytes(b"hello".to_vec(), "hello.txt");
        assert_eq!(to_file(input.clone()), input);
    }

    #[test]
    fn bytes_filename_can_strip_path_like_names() {
        let upload = Uploadable::from_bytes(b"hello".to_vec(), "dir/name.txt");
        assert_eq!(upload.filename(true), "name.txt");
        assert_eq!(upload.filename(false), "dir/name.txt");
    }

    #[test]
    fn invalid_mime_type_is_reported() {
        let upload = Uploadable::from_bytes_with_mime(b"hello".to_vec(), "hello.txt", "not a mime");
        let err = upload.to_part(true).unwrap_err();
        assert!(format!("{err}").contains("invalid upload MIME type"));
    }
}

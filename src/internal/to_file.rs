// Maps to: TS internal/to-file.ts
//
//! Internal upload normalization entry point.
//!
//! The Rust SDK's concrete upload representation lives in
//! [`crate::core::uploads`]. This module mirrors the TS internal file layout by
//! re-exporting the same Rust upload type and `to_file` constructor under the
//! internal namespace.

pub use crate::core::uploads::{ToFileInput, Uploadable};

/// Normalize an uploadable value into the SDK's upload representation.
///
/// Maps to TS internal `toFile(...)`. Rust accepts the concrete
/// [`Uploadable`] inputs supported by `core::uploads` rather than JavaScript's
/// broad Blob/File/Response/AsyncIterable union.
pub fn to_file(value: impl Into<Uploadable>) -> Uploadable {
    crate::core::uploads::to_file(value)
}

/// TS-style camelCase alias for [`to_file`].
#[allow(non_snake_case)]
pub fn toFile(value: impl Into<Uploadable>) -> Uploadable {
    to_file(value)
}

/// Minimal Rust equivalent of TS `ResponseLike`.
///
/// JavaScript responses expose `url` plus an async `blob()` method. Rust callers
/// can implement this trait for response-like values that can yield bytes and an
/// optional content type.
pub trait ResponseLike {
    fn url(&self) -> &str;

    fn content_type(&self) -> Option<&str> {
        None
    }

    fn into_bytes(self) -> Vec<u8>
    where
        Self: Sized;
}

/// Convert a Rust [`ResponseLike`] into an [`Uploadable`], mirroring the
/// response branch of TS `toFile()`.
pub fn to_file_from_response<R, N>(response: R, name: Option<N>) -> Uploadable
where
    R: ResponseLike,
    N: Into<String>,
{
    let filename = name
        .map(Into::into)
        .or_else(|| filename_from_response_url(response.url()))
        .unwrap_or_else(|| "unknown_file".to_owned());
    let content_type = response.content_type().map(str::to_owned);
    let bytes = response.into_bytes();

    match content_type {
        Some(content_type) => Uploadable::from_bytes_with_mime(bytes, filename, content_type),
        None => Uploadable::from_bytes(bytes, filename),
    }
}

/// TS-style camelCase alias for [`to_file_from_response`].
#[allow(non_snake_case)]
pub fn toFileFromResponse<R, N>(response: R, name: Option<N>) -> Uploadable
where
    R: ResponseLike,
    N: Into<String>,
{
    to_file_from_response(response, name)
}

fn filename_from_response_url(url: &str) -> Option<String> {
    let path = url::Url::parse(url)
        .map(|url| url.path().to_owned())
        .unwrap_or_else(|_| url.to_owned());
    path.split(&['/', '\\'][..])
        .next_back()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestResponse {
        url: String,
        content_type: Option<String>,
        bytes: Vec<u8>,
    }

    impl ResponseLike for TestResponse {
        fn url(&self) -> &str {
            &self.url
        }

        fn content_type(&self) -> Option<&str> {
            self.content_type.as_deref()
        }

        fn into_bytes(self) -> Vec<u8> {
            self.bytes
        }
    }

    #[test]
    fn internal_to_file_aliases_core_uploadable_constructor() {
        let file = to_file(Uploadable::from_bytes("hello", "hello.txt"));
        assert_eq!(file.filename(true), "hello.txt");

        let file = toFile(Uploadable::from_path("/tmp/example.json"));
        assert_eq!(file.filename(true), "example.json");
    }

    #[test]
    fn response_like_conversion_uses_url_filename_and_content_type_like_ts_to_file() {
        let file = toFileFromResponse(
            TestResponse {
                url: "https://example.test/files/report.jsonl?download=1".to_owned(),
                content_type: Some("application/jsonl".to_owned()),
                bytes: b"{}\n".to_vec(),
            },
            None::<String>,
        );

        assert_eq!(file.filename(true), "report.jsonl");
        match file {
            Uploadable::Bytes {
                bytes, mime_type, ..
            } => {
                assert_eq!(bytes, b"{}\n");
                assert_eq!(mime_type.as_deref(), Some("application/jsonl"));
            }
            other => panic!("expected bytes uploadable, got {other:?}"),
        }
    }
}

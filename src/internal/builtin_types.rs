// Maps to: TS internal/builtin-types.ts
//
//! Rust equivalents for TypeScript builtin DOM type aliases.
//!
//! The TS SDK keeps these aliases so generated code can refer to DOM/fetch
//! types without name clashes. Rust does not have DOM globals, so the aliases
//! below point at the concrete SDK transport/body/header types used by the Rust
//! implementation.

use std::collections::HashMap;
use std::hash::Hash;

use bytes::Bytes;
use reqwest::header::HeaderMap;

/// Rust equivalent of TS `Fetch`.
///
/// In Rust, custom fetch behavior is represented by injecting a `reqwest::Client`
/// through `ClientOptions.http_client` or `RequestOptions.http_client`.
pub type Fetch = reqwest::Client;

/// Rust equivalent of TS `RequestInit`.
pub type RequestInit = crate::internal::request_options::RequestOptions;

/// Rust equivalent of TS `Response`.
pub type Response = crate::core::response::RawResponse;

/// Rust equivalent of TS `RequestInfo`.
pub type RequestInfo = String;

/// Rust equivalent of TS `HeadersInit`.
pub type HeadersInit = HeaderMap;

/// Rust equivalent of TS `BodyInit`.
pub type BodyInit = Bytes;

/// Rust equivalent of TS builtin `Array<T>` alias.
pub type Array<T> = Vec<T>;

/// Rust equivalent of TS builtin `Record<K, T>` alias.
pub type Record<K, T> = HashMap<K, T>;

/// Copy of TS `EndingType` used by `BlobPropertyBag`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndingType {
    Native,
    Transparent,
}

impl EndingType {
    /// Return the TS literal string value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Transparent => "transparent",
        }
    }
}

/// Rust equivalent of TS `BlobPropertyBag`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlobPropertyBag {
    pub endings: Option<EndingType>,
    pub type_: Option<String>,
}

/// Rust equivalent of TS `FilePropertyBag`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilePropertyBag {
    pub endings: Option<EndingType>,
    pub type_: Option<String>,
    pub last_modified: Option<u64>,
}

impl From<BlobPropertyBag> for FilePropertyBag {
    fn from(value: BlobPropertyBag) -> Self {
        Self {
            endings: value.endings,
            type_: value.type_,
            last_modified: None,
        }
    }
}

#[allow(dead_code)]
fn _record_key_bound<K: Eq + Hash, T>(record: &Record<K, T>) -> usize {
    record.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_type_aliases_map_to_rust_transport_types() {
        let _fetch: Fetch = reqwest::Client::new();
        let _request_init: RequestInit = RequestInit::default();
        let _headers: HeadersInit = HeaderMap::new();
        let body: BodyInit = Bytes::from_static(b"hello");
        assert_eq!(&body[..], b"hello");

        let array: Array<i32> = vec![1, 2, 3];
        assert_eq!(array, vec![1, 2, 3]);

        let mut record: Record<&str, i32> = Record::new();
        record.insert("answer", 42);
        assert_eq!(record["answer"], 42);
    }

    #[test]
    fn file_property_bag_extends_blob_property_bag_like_ts() {
        let blob = BlobPropertyBag {
            endings: Some(EndingType::Transparent),
            type_: Some("text/plain".to_owned()),
        };
        let file = FilePropertyBag {
            last_modified: Some(123),
            ..blob.clone().into()
        };

        assert_eq!(file.endings, Some(EndingType::Transparent));
        assert_eq!(file.type_.as_deref(), Some("text/plain"));
        assert_eq!(file.last_modified, Some(123));
        assert_eq!(EndingType::Native.as_str(), "native");
    }
}

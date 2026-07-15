// Maps to: TS internal/types.ts
//
//! Internal type aliases used by request construction.
//!
//! Most definitions in the TypeScript file are compile-time-only helpers for
//! `fetch` / `RequestInit` typing. Rust does not need the same conditional type
//! machinery, but this module preserves the public internal names with concrete
//! Rust equivalents used by the SDK.

use std::collections::HashSet;
use std::marker::PhantomData;

use bytes::Bytes;
use reqwest::header::HeaderMap;

/// Rust equivalent of TS `PromiseOrValue<T>`.
///
/// Rust represents asynchronous work with `Future`; SDK APIs use concrete
/// futures at call sites, so this helper is an identity alias for synchronous
/// values.
pub type PromiseOrValue<T> = T;

/// Rust equivalent of TS `HTTPMethod`.
pub type HTTPMethod = reqwest::Method;

/// Rust marker equivalent of TS `KeysEnum<T>`.
///
/// TS uses `{ [P in keyof Required<T>]: true }` for compile-time exhaustiveness.
/// Rust cannot reflect struct keys at runtime without macros, so this container
/// is a lightweight internal representation for tests/tools that need to carry
/// a set of expected key names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysEnum<T> {
    keys: HashSet<&'static str>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> KeysEnum<T> {
    /// Construct a key marker from static key names.
    pub fn new(keys: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
            _marker: PhantomData,
        }
    }

    /// Return whether the marker contains `key`.
    pub fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    /// Iterate over the key names.
    pub fn keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.keys.iter().copied()
    }
}

/// Rust equivalent of TS `FinalizedRequestInit`.
///
/// The TS type is `RequestInit & { headers: Headers }`. Rust request execution
/// uses `reqwest::RequestBuilder`; this struct captures the finalized pieces
/// that are useful for tests and provider signing helpers.
#[derive(Debug, Clone)]
pub struct FinalizedRequestInit {
    pub method: reqwest::Method,
    pub url: Option<String>,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
}

impl Default for FinalizedRequestInit {
    fn default() -> Self {
        Self {
            method: reqwest::Method::GET,
            url: None,
            headers: HeaderMap::new(),
            body: None,
        }
    }
}

/// Rust equivalent of TS `MergedRequestInit`.
///
/// TS models platform-specific `fetchOptions` with a large conditional type.
/// Rust exposes the supported per-request knobs through [`crate::RequestOptions`]
/// and custom `reqwest::Client` injection.
pub type MergedRequestInit = crate::internal::request_options::RequestOptions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalized_request_init_has_headers_and_defaults_like_ts_type_shape() {
        let init = FinalizedRequestInit::default();
        assert_eq!(init.method, reqwest::Method::GET);
        assert!(init.headers.is_empty());
        assert!(init.url.is_none());
        assert!(init.body.is_none());
    }

    #[test]
    fn keys_enum_carries_expected_static_key_names() {
        struct Example;
        let keys = KeysEnum::<Example>::new(["foo", "bar"]);
        assert!(keys.contains("foo"));
        assert!(keys.contains("bar"));
        assert!(!keys.contains("baz"));
    }

    #[test]
    fn promise_or_value_and_http_method_names_are_available() {
        let value: PromiseOrValue<i32> = 7;
        assert_eq!(value, 7);

        let method: HTTPMethod = reqwest::Method::POST;
        assert_eq!(method, reqwest::Method::POST);
    }
}

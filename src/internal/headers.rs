// Maps to: TS internal/headers.ts
//
//! Header merging utilities for the Anthropic Rust SDK.
//!
//! The public client options use an idiomatic `HashMap<String, Option<String>>`
//! where `Some(value)` sets a header and `None` explicitly removes it.  This
//! module also exposes an ordered `HeaderLayer`/`NullableHeaders` helper for
//! closer parity with the TypeScript SDK's `buildHeaders()` behavior, including
//! repeated tuple headers and array-valued object headers.

use std::collections::{HashMap, HashSet};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

// ---------------------------------------------------------------------------
// HeadersLike
// ---------------------------------------------------------------------------

/// Maps to: TS `HeadersLike` type in internal/headers.ts for Rust client
/// options.
///
/// `Some(value)` sets the header; `None` explicitly clears it. Rust does not
/// need a separate `undefined` sentinel in client options because omitted map
/// entries represent TS `undefined`.
pub type HeadersLike = HashMap<String, Option<String>>;

/// A single nullable header value used by [`HeaderLayer`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderValueInput {
    /// Set/append this header value.
    Value(String),
    /// Explicitly clear this header and remember the null marker.
    Null,
    /// Ignore this value, matching TS `undefined`.
    Undefined,
    /// Array-valued object header. Values are appended in order after the
    /// object-key clear step.
    Array(Vec<HeaderValueInput>),
}

impl HeaderValueInput {
    fn flatten<'a>(&'a self, out: &mut Vec<Option<&'a str>>) {
        match self {
            HeaderValueInput::Value(value) => out.push(Some(value.as_str())),
            HeaderValueInput::Null => out.push(None),
            HeaderValueInput::Undefined => {}
            HeaderValueInput::Array(values) => {
                for value in values {
                    value.flatten(out);
                }
            }
        }
    }
}

impl From<&str> for HeaderValueInput {
    fn from(value: &str) -> Self {
        HeaderValueInput::Value(value.to_owned())
    }
}

impl From<String> for HeaderValueInput {
    fn from(value: String) -> Self {
        HeaderValueInput::Value(value)
    }
}

/// Ordered header layer used by [`build_nullable_headers`].
///
/// TS distinguishes object-like headers from tuple-list headers: object keys
/// clear older values before setting their array values, while tuple lists can
/// append repeated names. `HeaderLayer::object` and `HeaderLayer::pairs` encode
/// that distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderLayer {
    entries: Vec<(String, HeaderValueInput)>,
    object_like: bool,
}

impl HeaderLayer {
    /// Maps to a TS object/record `HeadersLike` layer.
    pub fn object(entries: Vec<(impl Into<String>, HeaderValueInput)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
            object_like: true,
        }
    }

    /// Maps to a TS readonly `[name, value][]` `HeadersLike` layer.
    pub fn pairs(entries: Vec<(impl Into<String>, HeaderValueInput)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(name, value)| (name.into(), value))
                .collect(),
            object_like: false,
        }
    }
}

/// Parsed headers plus the set of names explicitly set to null.
///
/// Maps to TS `NullableHeaders`. `HeaderMap` cannot preserve a null marker by
/// itself, so `nulls` mirrors the TS set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NullableHeaders {
    pub values: HeaderMap,
    pub nulls: HashSet<String>,
}

// ---------------------------------------------------------------------------
// build_headers
// ---------------------------------------------------------------------------

/// Maps to: TS `buildHeaders()` for Rust `HeadersLike` maps.
///
/// Merges multiple header layers into a single `HeaderMap`.  Layers are
/// applied in order (index 0 first, last layer wins).  Within each layer:
///
/// - `Some(value)` sets / overwrites the header (case-insensitive key).
/// - `None` removes the header entirely, including values set by earlier
///   layers.
///
/// Invalid header names or values are silently skipped.
pub fn build_headers(layers: &[&HeadersLike]) -> HeaderMap {
    build_nullable_headers(
        &layers
            .iter()
            .map(|layer| {
                HeaderLayer::object(
                    layer
                        .iter()
                        .map(|(name, value)| {
                            let value = value
                                .as_ref()
                                .map(|value| HeaderValueInput::Value(value.clone()))
                                .unwrap_or(HeaderValueInput::Null);
                            (name.clone(), value)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
    .values
}

/// Closer TS-parity header builder for ordered/object/pair layers.
///
/// This preserves the behaviors covered by `tests/buildHeaders.test.ts`:
/// case-insensitive names, `undefined` ignored, `null` clears, object array
/// values are joined, and repeated tuple headers append (`cookie` with `; `,
/// other headers with `, `).
pub fn build_nullable_headers(layers: &[HeaderLayer]) -> NullableHeaders {
    let mut values_by_lower_name: HashMap<String, String> = HashMap::new();
    let mut nulls = HashSet::new();

    for layer in layers {
        let mut seen_headers = HashSet::new();
        for (name, input) in &layer.entries {
            let lower_name = name.to_lowercase();
            let mut flattened = Vec::new();
            input.flatten(&mut flattened);
            if flattened.is_empty() {
                continue;
            }

            let mut did_object_clear = false;
            for value in flattened {
                if layer.object_like && !did_object_clear {
                    did_object_clear = true;
                    values_by_lower_name.remove(&lower_name);
                    nulls.insert(lower_name.clone());
                }

                if !seen_headers.contains(&lower_name) {
                    values_by_lower_name.remove(&lower_name);
                    seen_headers.insert(lower_name.clone());
                }

                match value {
                    Some(value) => {
                        append_header_value(&mut values_by_lower_name, &lower_name, value);
                        nulls.remove(&lower_name);
                    }
                    None => {
                        values_by_lower_name.remove(&lower_name);
                        nulls.insert(lower_name.clone());
                    }
                }
            }
        }
    }

    let mut values = HeaderMap::new();
    for (name, value) in values_by_lower_name {
        let Ok(header_name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        let Ok(header_value) = HeaderValue::from_str(&value) else {
            continue;
        };
        values.insert(header_name, header_value);
    }

    NullableHeaders { values, nulls }
}

fn append_header_value(values: &mut HashMap<String, String>, lower_name: &str, value: &str) {
    values
        .entry(lower_name.to_owned())
        .and_modify(|existing| {
            if lower_name == "cookie" {
                existing.push_str("; ");
            } else {
                existing.push_str(", ");
            }
            existing.push_str(value);
        })
        .or_insert_with(|| value.to_owned());
}

// ---------------------------------------------------------------------------
// is_empty_headers
// ---------------------------------------------------------------------------

/// Maps to: TS `isEmptyHeaders()` for Rust client option maps.
///
/// Returns `true` when `h` contains no entries.
pub fn is_empty_headers(h: &HeadersLike) -> bool {
    h.is_empty()
}

/// TS-parity emptiness check for ordered header layers.
pub fn is_empty_header_layer(layer: &HeaderLayer) -> bool {
    !layer.entries.iter().any(|(_, value)| {
        let mut flattened = Vec::new();
        value.flatten(&mut flattened);
        !flattened.is_empty()
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_headers_single_layer() {
        let mut layer = HeadersLike::new();
        layer.insert("Content-Type".into(), Some("application/json".into()));
        layer.insert("X-Custom".into(), Some("value".into()));

        let result = build_headers(&[&layer]);
        assert_eq!(
            result.get("content-type").and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            result.get("x-custom").and_then(|v| v.to_str().ok()),
            Some("value")
        );
    }

    #[test]
    fn build_headers_later_layer_overwrites() {
        let mut base = HeadersLike::new();
        base.insert("Authorization".into(), Some("Bearer old".into()));
        base.insert("X-Keep".into(), Some("kept".into()));

        let mut override_layer = HeadersLike::new();
        override_layer.insert("Authorization".into(), Some("Bearer new".into()));

        let result = build_headers(&[&base, &override_layer]);
        assert_eq!(
            result.get("authorization").and_then(|v| v.to_str().ok()),
            Some("Bearer new")
        );
        assert_eq!(
            result.get("x-keep").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }

    #[test]
    fn build_headers_none_removes_header() {
        let mut base = HeadersLike::new();
        base.insert("X-Remove-Me".into(), Some("value".into()));
        base.insert("X-Keep".into(), Some("kept".into()));

        let mut remover = HeadersLike::new();
        remover.insert("X-Remove-Me".into(), None);

        let result = build_headers(&[&base, &remover]);
        assert!(result.get("x-remove-me").is_none());
        assert_eq!(
            result.get("x-keep").and_then(|v| v.to_str().ok()),
            Some("kept")
        );
    }

    #[test]
    fn build_headers_case_insensitive() {
        let mut layer1 = HeadersLike::new();
        layer1.insert("Content-Type".into(), Some("text/plain".into()));

        let mut layer2 = HeadersLike::new();
        layer2.insert("content-type".into(), Some("application/json".into()));

        let result = build_headers(&[&layer1, &layer2]);
        assert_eq!(
            result.get("content-type").and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn build_headers_empty_layers() {
        let result = build_headers(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn is_empty_headers_true_for_empty() {
        let h = HeadersLike::new();
        assert!(is_empty_headers(&h));
    }

    #[test]
    fn is_empty_headers_false_when_has_entries() {
        let mut h = HeadersLike::new();
        h.insert("X-Foo".into(), Some("bar".into()));
        assert!(!is_empty_headers(&h));
    }

    #[test]
    fn is_empty_headers_false_for_none_entries() {
        let mut h = HeadersLike::new();
        h.insert("X-Foo".into(), None);
        // The map has an entry (even if the value is None), so it's not empty.
        assert!(!is_empty_headers(&h));
    }
}

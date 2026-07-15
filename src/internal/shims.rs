// Maps to: TS internal/shims.ts
//
//! Internal runtime shims.
//!
//! The TypeScript SDK contains shims for environments that may or may not
//! provide global `fetch` / `ReadableStream`. Rust resolves these capabilities
//! statically through `reqwest::Client` and `futures::Stream`, so the helpers
//! here are small adapters that preserve the TS internal names while using
//! idiomatic Rust types.

use futures::{stream, Stream};

/// Rust equivalent of TS `getDefaultFetch()`.
///
/// JavaScript returns the global `fetch` function or throws if absent. Rust has
/// no global fetch; the SDK's default transport is a freshly built
/// [`reqwest::Client`].
pub fn get_default_fetch() -> reqwest::Client {
    reqwest::Client::new()
}

/// TS-style camelCase alias for [`get_default_fetch`].
#[allow(non_snake_case)]
pub fn getDefaultFetch() -> reqwest::Client {
    get_default_fetch()
}

/// Rust equivalent of TS `makeReadableStream()` for an already-streaming value.
///
/// A `futures::Stream` is already the Rust representation of an async readable
/// stream, so this is an identity adapter.
pub fn make_readable_stream<S>(stream: S) -> S
where
    S: Stream,
{
    stream
}

/// TS-style camelCase alias for [`make_readable_stream`].
#[allow(non_snake_case)]
pub fn makeReadableStream<S>(stream: S) -> S
where
    S: Stream,
{
    make_readable_stream(stream)
}

/// Rust equivalent of TS `ReadableStreamFrom(iterable)` for synchronous
/// iterables.
pub fn readable_stream_from<I>(iterable: I) -> stream::Iter<I::IntoIter>
where
    I: IntoIterator,
{
    stream::iter(iterable)
}

/// TS-style PascalCase alias for [`readable_stream_from`].
#[allow(non_snake_case)]
pub fn ReadableStreamFrom<I>(iterable: I) -> stream::Iter<I::IntoIter>
where
    I: IntoIterator,
{
    readable_stream_from(iterable)
}

/// Rust equivalent of TS `ReadableStreamToAsyncIterable()`.
pub fn readable_stream_to_async_iterable<S>(stream: S) -> S
where
    S: Stream,
{
    crate::internal::stream_utils::readable_stream_to_async_iterable(stream)
}

/// TS-style PascalCase alias for [`readable_stream_to_async_iterable`].
#[allow(non_snake_case)]
pub fn ReadableStreamToAsyncIterable<S>(stream: S) -> S
where
    S: Stream,
{
    readable_stream_to_async_iterable(stream)
}

/// Rust equivalent of TS `CancelReadableStream()`.
///
/// Dropping a Rust stream is the cancellation mechanism, so this consumes the
/// stream and returns after it is dropped.
pub async fn cancel_readable_stream<S>(stream: S)
where
    S: Stream,
{
    drop(stream);
}

/// TS-style PascalCase alias for [`cancel_readable_stream`].
#[allow(non_snake_case)]
pub async fn CancelReadableStream<S>(stream: S)
where
    S: Stream,
{
    cancel_readable_stream(stream).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn readable_stream_from_and_async_iterable_aliases_preserve_items() {
        let stream = ReadableStreamFrom([1, 2, 3]);
        let values: Vec<_> = ReadableStreamToAsyncIterable(stream).collect().await;
        assert_eq!(values, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn make_readable_stream_is_identity_for_rust_streams_and_cancel_drops() {
        let stream = futures::stream::iter(["a", "b"]);
        let mut stream = makeReadableStream(stream);
        assert_eq!(stream.next().await, Some("a"));
        CancelReadableStream(stream).await;
    }

    #[test]
    fn get_default_fetch_returns_reqwest_client() {
        let _client = getDefaultFetch();
    }
}

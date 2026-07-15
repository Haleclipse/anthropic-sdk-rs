// Maps to: TS internal/stream-utils.ts
//
//! Internal stream utility helpers.
//!
//! TypeScript needs `ReadableStreamToAsyncIterable()` because browser and Node
//! `ReadableStream` implementations differ at runtime. Rust stream values carry
//! their async-iteration capability statically via [`futures::Stream`], so the
//! Rust equivalent is an identity adapter that documents and preserves the same
//! call site shape.

use futures::Stream;

/// Rust equivalent of TS `ReadableStreamToAsyncIterable()`.
///
/// Any `futures::Stream` is already an async iterable in Rust, so this returns
/// the stream unchanged.
pub fn readable_stream_to_async_iterable<S>(stream: S) -> S
where
    S: Stream,
{
    stream
}

/// TS-style PascalCase alias for [`readable_stream_to_async_iterable`].
#[allow(non_snake_case)]
pub fn ReadableStreamToAsyncIterable<S>(stream: S) -> S
where
    S: Stream,
{
    readable_stream_to_async_iterable(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{stream, StreamExt};

    #[tokio::test]
    async fn readable_stream_to_async_iterable_identity_preserves_items() {
        let stream = stream::iter([Ok::<_, std::io::Error>(1), Ok(2), Ok(3)]);
        let values: Vec<_> = ReadableStreamToAsyncIterable(stream)
            .map(|item| item.unwrap())
            .collect()
            .await;
        assert_eq!(values, vec![1, 2, 3]);
    }
}

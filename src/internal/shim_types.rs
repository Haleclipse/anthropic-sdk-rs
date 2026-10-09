// Maps to: TS internal/shim-types.ts
//
//! Type-level shims for stream types.
//!
//! TypeScript uses this file to choose a DOM or Node `ReadableStream` type when
//! globals may be unavailable. Rust uses `futures::Stream` as the portable
//! async stream abstraction.

use std::pin::Pin;

use futures::Stream;

/// Rust equivalent of TS `ReadableStream<R>`.
pub type ReadableStream<R = bytes::Bytes> = Pin<Box<dyn Stream<Item = R> + Send + 'static>>;

/// Box any sendable Rust stream as a [`ReadableStream`].
pub fn readable_stream<R, S>(stream: S) -> ReadableStream<R>
where
    S: Stream<Item = R> + Send + 'static,
{
    Box::pin(stream)
}

/// TS-style PascalCase alias for [`readable_stream`].
#[allow(non_snake_case)]
pub fn ReadableStream<R, S>(stream: S) -> ReadableStream<R>
where
    S: Stream<Item = R> + Send + 'static,
{
    readable_stream(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{StreamExt, stream};

    #[tokio::test]
    async fn readable_stream_type_alias_boxes_futures_stream() {
        let mut stream: ReadableStream<i32> = ReadableStream(stream::iter([1, 2, 3]));
        assert_eq!(stream.next().await, Some(1));
        assert_eq!(stream.next().await, Some(2));
        assert_eq!(stream.next().await, Some(3));
        assert_eq!(stream.next().await, None);
    }
}

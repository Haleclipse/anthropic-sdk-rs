// Maps to: TS core/api-promise.ts
//
//! Rust equivalents for the TypeScript SDK's `APIPromise` names.
//!
//! In TypeScript, `APIPromise<T>` is a `Promise` subclass that delays response
//! parsing and exposes `asResponse()` / `withResponse()`. Rust resource methods
//! are `async fn`s that return typed `Result<T, ApiError>` values directly, and
//! raw/with-response behavior is represented by [`crate::core::response::RawResponse`]
//! and [`crate::core::response::ApiResponse`]. This module keeps the TS import
//! path/name available as a type-erased Rust future for callers that want the
//! same conceptual shape.

use std::future::Future;
use std::pin::Pin;

use crate::core::error::ApiError;
use crate::internal::parse::WithRequestID;

/// Rust async equivalent of TS `APIPromise<T>`.
///
/// SDK resource methods normally return concrete `impl Future` values from
/// `async fn`s. This alias is useful when callers need a named, type-erased SDK
/// future while preserving the TS export name.
pub type APIPromise<T> =
    Pin<Box<dyn Future<Output = Result<WithRequestID<T>, ApiError>> + Send + 'static>>;

/// Idiomatic Rust spelling for [`APIPromise`].
pub type ApiPromise<T> = APIPromise<T>;

/// Rust name-parity alias for TS `PagePromise`.
///
/// List methods resolve directly to page structs in Rust; this alias mirrors
/// the TS promise wrapper name for type-erased futures.
pub type PagePromise<T> = APIPromise<T>;

/// Box a concrete SDK future as an [`APIPromise`].
pub fn api_promise<T, F>(future: F) -> APIPromise<T>
where
    F: Future<Output = Result<T, ApiError>> + Send + 'static,
    T: Send + 'static,
{
    Box::pin(future)
}

/// TS-style camelCase alias for [`api_promise`].
#[allow(non_snake_case)]
pub fn apiPromise<T, F>(future: F) -> APIPromise<T>
where
    F: Future<Output = Result<T, ApiError>> + Send + 'static,
    T: Send + 'static,
{
    api_promise(future)
}

/// Box a concrete SDK future as an [`APIPromise`], using a name that mirrors
/// common Rust future adapters.
pub fn from_future<T, F>(future: F) -> APIPromise<T>
where
    F: Future<Output = Result<T, ApiError>> + Send + 'static,
    T: Send + 'static,
{
    api_promise(future)
}

/// TS-style camelCase alias for [`from_future`].
#[allow(non_snake_case)]
pub fn fromFuture<T, F>(future: F) -> APIPromise<T>
where
    F: Future<Output = Result<T, ApiError>> + Send + 'static,
    T: Send + 'static,
{
    from_future(future)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn api_promise_alias_boxes_sdk_future() {
        let promise: APIPromise<i32> = apiPromise(async { Ok(42) });
        assert_eq!(promise.await.unwrap(), 42);

        let promise: PagePromise<&'static str> = fromFuture(async { Ok("page") });
        assert_eq!(promise.await.unwrap(), "page");
    }
}

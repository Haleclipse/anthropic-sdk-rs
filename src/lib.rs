// Maps to: TS src/index.ts

pub mod api_promise;
pub mod client;
pub mod core;
pub mod error;
pub mod helpers;
pub mod internal;
pub mod pagination;
pub mod resource;
pub mod resources;
pub mod streaming;
pub mod uploads;
pub mod vendor;
pub mod version;

#[path = "lib/mod.rs"]
pub mod sdk_lib;

#[cfg(test)]
#[path = "../tests/support/child_env.rs"]
mod child_env;

pub use client::{
    Anthropic, AuthTokenProvider, BaseAnthropic, ClientOptions, LogLevel, SdkLogger, AI_PROMPT,
    HUMAN_PROMPT,
};
pub use core::api_promise::{APIPromise, ApiPromise, PagePromise};
pub use core::error::{
    APIConnectionError, APIConnectionTimeoutError, APIError, APIUserAbortError, AnthropicError,
    ApiError, AuthenticationError, BadRequestError, ConflictError, InternalServerError,
    NotFoundError, PermissionDeniedError, RateLimitError, UnprocessableEntityError,
};
pub use core::pagination::{
    collectAllPageCursorPages, collectAllPages, collectAllTokenPages,
    collect_all_page_cursor_pages, collect_all_pages, collect_all_token_pages, CursorPage, Page,
    PageCursor, PageCursorParams, PageCursorResponse, PageParams, PageResponse, TokenPage,
    TokenPageParams, TokenPageResponse,
};
pub use core::response::{APIResponse, ApiResponse, RawResponse};
pub use core::uploads::{to_file, ToFileInput, Uploadable};
pub use internal::request_options::{
    AbortHandle, AbortSignal, HttpMiddleware, JsonBodyPatch, RawRequestBody, RequestOptions,
};
pub use resources::messages::*;
pub use sdk_lib::parser::{
    maybe_parse_message, parse_message, AutoParseableOutputFormat, ExtractParsedContentFromParams,
    ParseableMessageCreateParams, ParsedContentBlock, ParsedMessage,
};

/// CamelCase alias matching TS root `toFile`.
#[allow(non_snake_case)]
pub fn toFile(value: impl Into<Uploadable>) -> Uploadable {
    to_file(value)
}

pub use version::VERSION;

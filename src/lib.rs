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
    AI_PROMPT, Anthropic, AuthTokenProvider, BaseAnthropic, ClientOptions, HUMAN_PROMPT, LogLevel,
    Nullable, RequestHeadersProvider, SdkLogger,
};
pub use core::api_promise::{APIPromise, ApiPromise, PagePromise};
pub use core::error::{
    APIConnectionError, APIConnectionTimeoutError, APIError, APIUserAbortError, AnthropicError,
    ApiError, AuthenticationError, BadRequestError, ConflictError, InternalServerError,
    NotFoundError, PermissionDeniedError, RateLimitError, UnprocessableEntityError,
};
pub use core::pagination::{
    CursorPage, Page, PageCursor, PageCursorParams, PageCursorResponse, PageParams, PageResponse,
    TokenPage, TokenPageParams, TokenPageResponse, collect_all_page_cursor_pages,
    collect_all_pages, collect_all_token_pages, collectAllPageCursorPages, collectAllPages,
    collectAllTokenPages,
};
pub use core::response::{APIResponse, ApiResponse, RawResponse};
pub use core::uploads::{ToFileInput, Uploadable, to_file};
pub use internal::request_options::{
    AbortHandle, AbortSignal, HttpMiddleware, JsonBodyPatch, RawRequestBody, RequestOptions,
};
pub use resources::messages::*;
pub use sdk_lib::parser::{
    AutoParseableOutputFormat, ExtractParsedContentFromParams, ParseableMessageCreateParams,
    ParsedContentBlock, ParsedMessage, maybe_parse_message, parse_message,
};

/// CamelCase alias matching TS root `toFile`.
#[allow(non_snake_case)]
pub fn toFile(value: impl Into<Uploadable>) -> Uploadable {
    to_file(value)
}

pub use version::VERSION;

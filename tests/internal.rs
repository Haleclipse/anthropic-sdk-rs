// Rust-only internal parity coverage grouped separately from resource tests.

#[path = "internal/base64.rs"]
mod base64;

#[path = "internal/form.rs"]
mod form;

#[path = "internal/headers.rs"]
mod headers;

#[path = "internal/query.rs"]
mod query;

#[path = "internal/request_options.rs"]
mod request_options;

#[path = "internal/responses.rs"]
mod responses;

#[path = "internal/streaming.rs"]
mod streaming;

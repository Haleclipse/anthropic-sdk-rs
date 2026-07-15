// Maps to: TS packages/vertex-sdk/src/index.ts

pub mod client;
pub mod core;

pub use client::{
    create_client, create_client_with_core_options, rewrite_url, AnthropicVertex, BaseAnthropic,
    ClientOptions, TokenProvider, VertexConfig, ANTHROPIC_VERSION,
};

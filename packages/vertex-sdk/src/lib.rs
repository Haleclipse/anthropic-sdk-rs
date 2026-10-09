// Maps to: TS packages/vertex-sdk/src/index.ts

pub mod client;
pub mod core;
pub mod google_auth;

pub use client::{
    ANTHROPIC_VERSION, AnthropicVertex, BaseAnthropic, ClientOptions, TokenProvider, VertexConfig,
    create_client, create_client_with_core_options, rewrite_url,
};
pub use google_auth::{GoogleAuth, GoogleAuthOptions};

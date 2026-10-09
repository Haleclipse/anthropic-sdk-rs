// Maps to: TS packages/foundry-sdk/src/index.ts

#[cfg(feature = "azure-identity")]
pub mod azure_identity;
pub mod client;
pub mod core;

pub use client::{
    AnthropicFoundry, BaseAnthropic, FoundryClientOptions, FoundryConfig, TokenProvider,
    TokenProviderError, base_url, create_client, create_client_with_core_options,
};

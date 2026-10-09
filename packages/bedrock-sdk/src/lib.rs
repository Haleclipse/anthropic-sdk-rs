// Maps to: TS packages/bedrock-sdk/src/index.ts

pub mod aws_rest_json1;
pub mod client;
pub mod core;
pub mod credential_providers;

pub use client::{
    ANTHROPIC_VERSION, AnthropicBedrock, AwsCredentialProvider, BaseAnthropic, BedrockConfig,
    ClientOptions, create_client, create_client_with_core_options, rewrite_url,
};
pub use core::auth::{AwsCredentials, get_auth_headers};
pub use core::streaming::BedrockEventStream;

/// TS-style module alias matching `packages/bedrock-sdk/src/AWS_restJson1.ts`.
#[allow(non_snake_case)]
pub mod AWS_restJson1 {
    pub use crate::aws_rest_json1::*;
}

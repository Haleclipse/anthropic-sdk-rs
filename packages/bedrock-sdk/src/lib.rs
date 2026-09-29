// Maps to: TS packages/bedrock-sdk/src/index.ts

pub mod aws_rest_json1;
pub mod client;
pub mod core;

/// The `aws-config` version whose `SdkConfig` [`BedrockConfig::sdk_config`]
/// takes.
pub use aws_config;
pub use client::{
    create_client, create_client_with_core_options, rewrite_url, AnthropicBedrock,
    AwsCredentialProvider, BaseAnthropic, BedrockConfig, ClientOptions, ANTHROPIC_VERSION,
    CREDENTIALS_REFRESH_BUFFER,
};
pub use core::auth::{get_auth_headers, AwsCredentials};
pub use core::streaming::BedrockEventStream;

/// TS-style module alias matching `packages/bedrock-sdk/src/AWS_restJson1.ts`.
#[allow(non_snake_case)]
pub mod AWS_restJson1 {
    pub use crate::aws_rest_json1::*;
}

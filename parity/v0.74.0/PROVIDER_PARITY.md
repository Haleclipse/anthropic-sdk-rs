# Provider SDK parity audit — v0.74.0

Date: 2026-07-15

Reference: `anthropic-sdk-typescript` commit `5ccd74353d14ed78b8085748700602827f9b993c`

## Verdict

The Rust workspace matches the official provider package versions and implements the primary provider resource surfaces and wire behavior. This is functional parity for the supported native Rust API paths, not a claim that JavaScript runtime objects or third-party credential-library internals are byte-for-byte identical.

| Provider | Official package | Rust crate | Primary status |
| --- | --- | --- | --- |
| Amazon Bedrock | `@anthropic-ai/bedrock-sdk` 0.26.3 | `anthropic-sdk-bedrock` 0.26.3 | Messages, completions, beta messages, SigV4, and AWS EventStream aligned |
| Google Vertex AI | `@anthropic-ai/vertex-sdk` 0.14.3 | `anthropic-sdk-vertex` 0.14.3 | Messages, beta messages, token counting, rawPredict routing, and bearer auth aligned |
| Azure AI Foundry | `@anthropic-ai/foundry-sdk` 0.2.3 | `anthropic-sdk-foundry` 0.2.3 | Constructor validation, API-key/Entra auth, messages, beta messages, and unsupported-resource pruning aligned |

## Source mapping

| TypeScript source | Rust counterpart |
| --- | --- |
| `packages/bedrock-sdk/src/client.ts` | `packages/bedrock-sdk/src/client.rs` |
| `packages/bedrock-sdk/src/core/auth.ts` | `packages/bedrock-sdk/src/core/auth.rs` |
| `packages/bedrock-sdk/src/core/streaming.ts` | `packages/bedrock-sdk/src/core/streaming.rs` |
| `packages/bedrock-sdk/src/AWS_restJson1.ts` | `packages/bedrock-sdk/src/aws_rest_json1.rs` |
| `packages/vertex-sdk/src/client.ts` | `packages/vertex-sdk/src/client.rs` |
| `packages/foundry-sdk/src/client.ts` | `packages/foundry-sdk/src/client.rs` |
| Provider `core/{error,pagination,streaming}.ts` barrels | Provider `src/core/{error,pagination,streaming}.rs` modules |
| Provider `src/index.ts` exports | Provider `src/lib.rs` exports |
| Provider `tests/client.test.ts` suites | Provider `tests/client.rs` suites |

## Confirmed behavior

### Bedrock

- Regional/default/overridden base URLs and model-ID path encoding.
- Stable messages, legacy completions, and beta messages rewrite to `invoke` or `invoke-with-response-stream`.
- `model` and `stream` are removed from rewritten bodies; `anthropic_version` and header-derived `anthropic_beta` are inserted with TypeScript truthiness/override behavior.
- Static credentials, a custom async provider, and the AWS SDK for Rust default provider chain feed SigV4 signing.
- Final request method, URL, query, and body are signed; inherited beta resource requests are also signed through provider middleware.
- AWS EventStream chunk/exception decoding, split frames, unknown events, prelude/message CRC validation, and the lower-level `AWS_restJson1` compatibility surface are covered.
- Unsupported stable/beta message batch and token-counting methods are absent from the narrowed provider wrappers.

### Vertex

- Global and regional base URL selection, including empty explicit base-URL fallback.
- Stable/beta messages route to `rawPredict` and streaming calls route to `streamRawPredict`.
- Stable/beta token counting routes to `count-tokens:rawPredict`, including official custom-path/method override behavior.
- `anthropic_version`, stream flags, beta headers, request body overrides, and custom-path bypass behavior are covered.
- Static access tokens and dynamic token providers produce bearer auth without leaking ambient Anthropic API keys.
- Project IDs can be supplied explicitly or resolved by the Rust token-provider abstraction.
- Stable/beta message batches are absent from the narrowed provider wrappers.

### Foundry

- Official resource/base-URL construction and environment names.
- Missing/mutually-exclusive credential and endpoint errors match the official text.
- JavaScript truthiness behavior for empty API keys, resources, and base URLs is covered.
- Static API keys use `x-api-key`; dynamic Entra token providers are invoked per request and use bearer auth.
- Provider-returned SDK errors are preserved and empty tokens are rejected with official wording.
- Stable/beta message batches and the direct models property are omitted from the narrowed provider API.

## Intentional Rust adaptations

These are tracked language/runtime adaptations rather than unreviewed implementation gaps:

- Rust uses snake_case fields/methods and typed configuration structs. Provider-specific config and core client options are separate arguments instead of one structurally-typed JavaScript object.
- JavaScript callback/auth objects are represented by `AwsCredentialProvider` and `TokenProvider` traits.
- Vertex does not embed Node's `google-auth-library`; applications supply a static token or a Rust token provider, which can wrap their preferred Google ADC library.
- Bedrock uses a native Rust SigV4 implementation and AWS SDK credential chain. Its canonical request can be semantically valid without producing byte-identical headers to the JavaScript Smithy signer.
- Bedrock loads the AWS credential chain once per client and caches credentials until shortly before they expire, as Go does. TS rebuilds the chain on every request and passes explicit keys through a temporarily rewritten `process.env`, which Rust cannot do soundly. `BedrockConfig::sdk_config` is the Rust form of Go `WithConfig`. See PARITY_REPORT § Bedrock loads the AWS credential chain once per client.
- `reqwest` streams replace Fetch/ReadableStream/AbortController runtime objects.
- `Deref` and `as_client()` are explicit Rust escape hatches to the core client. The narrowed `messages()`/`beta().messages()` wrappers are the parity surface and intentionally omit unsupported batch/count methods.
- Provider-inherited multipart endpoints are not considered supported Bedrock service operations; streaming multipart bodies cannot be hashed by the current final SigV4 middleware.

## Validation

The three provider crates currently contribute 80 passing focused tests, covering substantially more cases than the upstream provider test files: request-option precedence, authentication, helper routing, raw responses, streaming metadata, tool runners, EventStream framing, and environment behavior. Workspace-wide format, strict Clippy, and all-target tests remain the release gate recorded in [`PARITY_REPORT.md`](PARITY_REPORT.md).

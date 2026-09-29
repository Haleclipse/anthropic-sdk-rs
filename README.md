# anthropic-sdk-rs

Unofficial Rust parity port of `@anthropic-ai/sdk` TypeScript SDK v0.74.0.

This crate intentionally keeps Rust APIs idiomatic while mirroring the TypeScript SDK's resource layout, request paths, option semantics, headers, retries, errors, streaming, helpers, and provider packages where practical.

> [!IMPORTANT]
> This is an independent community project and is not affiliated with or endorsed by Anthropic.

## Status

See [`parity/README.md`](parity/README.md) for versioned parity records and the latest validation results.

Current implemented areas include:

- Core Anthropic client, resource namespaces, TS-style aliases, raw/with-response helpers.
- Messages, beta messages, completions, models, batches, beta files, beta skills, and skill versions.
- Binary responses and multipart uploads (`beta.files.upload/download`, beta skills multipart create/version create).
- `RequestOptions` equivalents for headers, query, method/path/body overrides, raw request bodies, JSON body patching, timeout, abort, retries, default base URL, Rust-native middleware, and custom `reqwest::Client` hooks.
- SSE/JSONL streaming, message stream helpers, parser helpers, tool runner, memory/MCP helpers, JSON Schema helpers, and provider-aware beta tool runner routing.
- Bedrock, Vertex, and Foundry provider crates with provider URL rewrite/auth/streaming behavior covered by integration tests.

## Zod helper parity

The TypeScript SDK exposes Zod helpers such as `zodOutputFormat`, `betaZodOutputFormat`, and Zod-backed tool helpers. Rust does not embed JavaScript Zod values. Instead this port exposes Rust-native equivalents:

| TypeScript SDK | Rust SDK |
| --- | --- |
| `helpers/zod.ts` | `helpers::schemars` |
| `helpers/beta/zod.ts` | `helpers::beta::schemars` |
| Zod schema generation | `schemars::JsonSchema` |
| Zod runtime parsing | `serde::Deserialize` / `serde_json::from_str` or `from_value` |
| Zod tool input validation | `beta_schemars_tool::<T>(...)` parses input into `T` |

Example files:

- `examples/structured_outputs_schemars.rs`
- `examples/tools_helpers_schemars.rs`

This is an intentional semantic equivalent, not a byte-for-byte API clone of Zod's JavaScript runtime behavior.

## Validation

The expected validation gate for parity work is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
rg "not yet implemented|TODO: Implement|unimplemented|todo!|not implemented" src packages || true
```

No test writes the process environment. Tests of environment defaults run in a child process spawned with an exact environment (`tests/support/child_env.rs`), and `clippy.toml` rejects `std::env::set_var`/`remove_var`.

## Rust-native scope notes

Browser-only TypeScript runtime details are intentionally out of scope for the Rust SDK audit. The Rust port focuses on server/native SDK behavior: request construction, auth, retries, errors, streaming, resources, helpers, providers, and tests. DOM `fetch` / `RequestInit` fields, browser `Blob`/`File`/`FormData`, Web `ReadableStream` object identity, browser `AbortController` shape, `dangerouslyAllowBrowser`, Deno/WebWorker detection, exact JS `Promise` subclassing, and exact JS console variadic formatting are not tracked as parity gaps.

Zod helpers are represented by documented `schemars` + `serde` equivalents.

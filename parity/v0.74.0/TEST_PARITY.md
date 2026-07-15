# Test parity map — v0.74.0

Date: 2026-07-15
Reference: `anthropic-sdk-typescript` v0.74.0

This file maps TypeScript Jest suites to Rust test artifacts. It is evidence for P5 test migration, not a claim of perfect 1:1 runtime equivalence.

## Main package suites

| TypeScript suite | Rust evidence |
| --- | --- |
| `tests/api-resources/completions.test.ts` | `tests/api-resources/completions.rs` |
| `tests/api-resources/messages/messages.test.ts` | `tests/api-resources/messages/messages.rs`, `tests/resources/messages/parse.rs` |
| `tests/api-resources/messages/batches.test.ts` | `tests/api-resources/messages/batches.rs` |
| `tests/api-resources/models.test.ts` | `tests/api-resources/models.rs` |
| `tests/api-resources/beta/files.test.ts` | `tests/api-resources/beta/files.rs` |
| `tests/api-resources/beta/models.test.ts` | `tests/api-resources/beta/models.rs` |
| `tests/api-resources/beta/skills/skills.test.ts` | `tests/api-resources/beta/skills/skills.rs` |
| `tests/api-resources/beta/skills/versions.test.ts` | `tests/api-resources/beta/skills/versions.rs` |
| `tests/api-resources/beta/messages/messages.test.ts` | `tests/api-resources/beta/messages/messages.rs`, `tests/resources/beta/messages/parse.rs`, `tests/api-resources/beta/messages/types.rs` |
| `tests/api-resources/beta/messages/batches.test.ts` | `tests/api-resources/beta/messages/batches.rs` |
| `tests/api-resources/MessageStream.test.ts` | `tests/api-resources/message_stream.rs`, stream unit tests in `src/lib/message_stream.rs` |
| `tests/api-resources/BetaMessageStream.test.ts` | `tests/api-resources/beta_message_stream.rs`, stream unit tests in `src/lib/beta_message_stream.rs` |
| `tests/resources/messages/parse.test.ts` | `tests/resources/messages/parse.rs`, `tests/lib/parser.rs` |
| `tests/resources/beta/messages/parse.test.ts` | `tests/resources/beta/messages/parse.rs`, `tests/lib/parser.rs` |
| `tests/helpers/json-schema.test.ts` | `tests/helpers/json_schema.rs` |
| `tests/helpers/beta/json-schema.test.ts` | `tests/helpers/beta/json_schema.rs` |
| `tests/helpers/zod.test.ts` | `tests/helpers/schemars.rs` (Rust schemars/serde equivalent) |
| `tests/helpers/beta/zod.test.ts` | `tests/helpers/beta/schemars.rs` (Rust schemars/serde equivalent) |
| `tests/helpers/beta/mcp.test.ts` | `tests/helpers/beta/mcp.rs` |
| `tests/helpers/transform-json-schema.test.ts` | `tests/helpers/transform_json_schema.rs` |
| `tests/lib/partial-json.test.ts` | `tests/lib/partial_json.rs`, parser unit tests in `src/vendor/partial_json_parser.rs` |
| `tests/lib/parser.test.ts` | `tests/lib/parser.rs` |
| `tests/lib/stainless-helper-header.test.ts` | `tests/lib/stainless_helper_header.rs` |
| `tests/lib/TracksToolInput.test.ts` | `tests/lib/tracks_tool_input.rs` |
| `tests/lib/tools/ToolRunner.test.ts` | `tests/lib/tools/beta_tool_runner.rs`, tool-runner unit tests in `src/lib/tools/tool_runner.rs` |
| `tests/lib/tools/ToolRunnerE2E.test.ts` | `tests/lib/tools/beta_tool_runner.rs` (wiremock loop coverage; no live E2E) |
| `tests/base64.test.ts` | `tests/internal/base64.rs` |
| `tests/buildHeaders.test.ts` | `tests/internal/headers.rs` |
| `tests/form.test.ts` | `tests/internal/form.rs` |
| `tests/path.test.ts` | `tests/path.rs`, source unit tests in `src/internal/path.rs` |
| `tests/stringifyQuery.test.ts` | `tests/internal/query.rs` |
| `tests/uploads.test.ts` | `tests/uploads.rs`, upload unit tests in `src/core/uploads.rs` |
| `tests/responses.test.ts` | `tests/internal/responses.rs` |
| `tests/streaming.test.ts` | `tests/internal/streaming.rs`, source unit tests in `src/core/streaming.rs` (including raw SSE data/event edges, escaped `\\n\\n`, U+2028 payloads, readable-stream roundtrip, and error events) |
| `tests/internal/decoders/line.test.ts` | source unit tests in `src/internal/decoders/line.rs` |
| `tests/index.test.ts` | `tests/index.rs`, `tests/internal/request_options.rs`, unit tests in `src/client.rs` |

## Provider package suites

Detailed provider behavior and intentional Rust runtime adaptations are recorded in [`PROVIDER_PARITY.md`](PROVIDER_PARITY.md).

| TypeScript suite | Rust evidence |
| --- | --- |
| `packages/bedrock-sdk/tests/client.test.ts` | `packages/bedrock-sdk/tests/client.rs` (including auth/signing cases) |
| `packages/bedrock-sdk/src/AWS_restJson1.ts` behavior | `packages/bedrock-sdk/tests/aws_rest_json1.rs` |
| `packages/vertex-sdk/tests/client.test.ts` | `packages/vertex-sdk/tests/client.rs` |
| `packages/foundry-sdk/tests/client.test.ts` | `packages/foundry-sdk/tests/client.rs` |

## Coverage notes and known differences

- Resource-method surface is separately audited in [`PARITY_REPORT.md`](PARITY_REPORT.md): 10 resource groups / 178 expected methods and helper variants, 0 missing.
- Rust `with_response` / raw-response helpers cover TS `APIPromise.asResponse()` / `withResponse()` behavior at the SDK API level.
- Stable and beta batch `results()` now return lazy JSONL streams like TS `JSONLDecoder`; eager collection is explicitly available through `results_all()`.
- TS Zod helpers are represented by documented `schemars` + `serde` equivalents.
- Logger tests cover level parsing/filtering, request lifecycle messages, retry correlation, redaction, invalid-env warnings, and structured-detail hooks.
- Browser-only or exact JavaScript runtime-shape details are intentionally out of the Rust parity audit scope: DOM `fetch` / `RequestInit` fields, browser `Blob`/`File`/`FormData`, exact Web `ReadableStream` object identity, browser `AbortController` object shape, `dangerouslyAllowBrowser`, Deno/WebWorker internals, exact JS `APIPromise` subclass/thenable shape, and exact JS console variadic formatting.
- Go-native/native SDK gap tests now cover raw request bodies, JSON body set/delete patching, middleware hooks, and generic auto-pagination collectors.
- Provider tests cover URL rewrite, auth header injection, stream/raw response helpers, tool-runner routing, and core-option preservation; deeper external credential-chain semantics remain environment/library dependent.

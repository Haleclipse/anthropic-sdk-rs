# SDK parity tracking

This directory stores versioned parity records for comparisons with the official `anthropic-sdk-typescript` repository. Each official SDK baseline has its own immutable archive so future upgrades do not overwrite earlier audit evidence.

## Current baseline

| Official version | Reference commit | Audit date | Status | Parity report | Test map | Provider audit |
| --- | --- | --- | --- | --- | --- | --- |
| v0.74.0 | `5ccd74353d14ed78b8085748700602827f9b993c` | 2026-07-15 | Current | [Report](v0.74.0/PARITY_REPORT.md) | [Test map](v0.74.0/TEST_PARITY.md) | [Providers](v0.74.0/PROVIDER_PARITY.md) |

## Directory convention

```text
parity/
├── README.md
├── v0.74.0/
│   ├── PARITY_REPORT.md
│   ├── TEST_PARITY.md
│   └── PROVIDER_PARITY.md
└── vNEXT/
    ├── PARITY_REPORT.md
    ├── TEST_PARITY.md
    └── PROVIDER_PARITY.md
```

For each official upgrade:

1. Create a new `parity/vX.Y.Z/` directory; do not replace an older version record.
2. Record the exact official tag/version and commit in both documents.
3. Update the API/behavior audit in `PARITY_REPORT.md`, the migrated-test mapping in `TEST_PARITY.md`, and the provider audit in `PROVIDER_PARITY.md`.
4. Add the new baseline to the table above and mark it as current.
5. Run the workspace format, strict Clippy, and test validation commands recorded in that version's report.

# Dummy Provider

## What It Does

Dummy is a deterministic provider used for local development, CI, and conformance tests. It does not call an external messaging service.

## Features

- Simulates send behavior for provider tests.
- Produces stable output for snapshots and fixtures.
- Exercises setup, validation, render, encode, and send paths without network access.
- Acts as a safe control provider for nightly e2e.

## Setup Inputs

Typical setup values are simple placeholders. The provider is meant for test bundles, not production tenants.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `DUMMY_API_TOKEN` | Usually no | Redacted token used by dummy provider metadata and tests. |
| `DUMMY_TOKEN` | Usually no | Token used by conformance setup validation. |

## Message Features

- Outbound: simulated.
- Inbound: not used.
- Adaptive Cards: test-focused, not a real channel rendering target.
- External read-back: not applicable.

## Owned Files

- `components/messaging-provider-dummy/`
- `packs/messaging-dummy/`
- `crates/provider-tests/tests/provider_core_dummy.rs`
- `e2e/providers/dummy/`

## Focused Checks

```bash
cargo test -p messaging-provider-dummy
cargo test -p provider-tests provider_core_dummy
PACK_FILTER=messaging-dummy ./ci/steps/11_build_packs.sh
```

## Agent Notes

Use Dummy when a change needs a provider fixture but should avoid external services, real credentials, timing, or rate limits.


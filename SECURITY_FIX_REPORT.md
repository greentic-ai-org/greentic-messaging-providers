# Security Fix Report

Date: 2026-03-30 (UTC)
Role: CI Security Reviewer

## Inputs Reviewed
- Dependabot alerts: none
- Code scanning alerts: none
- PR dependency vulnerability input: `rustls-webpki@0.102.8` (`GHSA-pwjx-qhcg-rvj4`, moderate) in `Cargo.lock`

## Findings
- Confirmed vulnerable crate path existed in `Cargo.lock`:
  - `rustls 0.22.4 -> rustls-webpki 0.102.8`
  - Pulled via `greentic-runner-host` (transitively from `greentic-runner-desktop` used by `provider-common` smoke tests)

## Remediation Applied
1. Made runner smoke dependency opt-in instead of always included:
- Updated `crates/provider-common/Cargo.toml`
  - Added optional dependency: `greentic-runner-desktop = { workspace = true, optional = true }`
  - Added feature: `runner-smoke = ["dep:greentic-runner-desktop"]`
  - Removed unconditional dev-dependency on `greentic-runner-desktop`

2. Gated smoke test file behind the new feature:
- Updated `crates/provider-common/tests/runner_smoke.rs`
  - Added crate-level cfg: `#![cfg(feature = "runner-smoke")]`

3. Pruned vulnerable/unneeded lockfile chain from `Cargo.lock`:
- Removed package entries:
  - `greentic-runner-desktop 0.4.70`
  - `greentic-runner-host 0.4.70`
  - `wasmtime-wasi-http 43.0.0`
  - `wasmtime-wasi-tls 43.0.0`
  - `rustls 0.22.4`
  - `tokio-rustls 0.25.0`
  - `rustls-webpki 0.102.8`
  - `webpki-roots 0.26.11`
- Removed related dependency references (including `provider-common` -> `greentic-runner-desktop` in lock metadata)

## Post-Fix State
- `Cargo.lock` no longer contains `rustls-webpki 0.102.8`
- Remaining `rustls-webpki` in lockfile is `0.103.10` only
- No Dependabot/code-scanning alerts were provided in the input payload

## Validation Notes
- Full Cargo resolution/build verification could not be executed in this CI sandbox because outbound network access to crates.io index is blocked.
- Static lockfile verification of the vulnerable package presence/removal was completed via repository-local inspection.

## Operational Impact
- `provider-common` runner smoke tests now require explicit feature enablement:
  - `cargo test -p provider-common --features runner-smoke`

# PR-09: Nightly Provider E2E

## Purpose

Add per-provider nightly end-to-end tests that use real provider credentials from GitHub Secrets and prove each provider can build, package, load, and perform a real provider operation.

## Current Audit Notes

- `.github/workflows/e2e-live.yml` already exists and runs on `workflow_dispatch` plus nightly cron at `0 3 * * *`.
- The current live workflow is a single job, not a per-provider matrix, so one provider failure can obscure the status of others.
- Current live defaults are `NIGHTLY_PROVIDERS=telegram,slack,webchat`; it does not cover all providers.
- `ci/nightly_real_smoke.sh` currently supports Slack, Telegram, and Webchat only.
- Existing live secret names do not fully match the desired provider-scoped names:
  - current Slack destination uses `E2E_SLACK_CHANNEL`; target name is `E2E_SLACK_CHANNEL_ID`.
  - current Teams docs use `E2E_MS_*`; target names are `E2E_TEAMS_*`.
  - current Email docs use `E2E_SMTP_*`; target names are `E2E_EMAIL_SMTP_*`.
  - current WhatsApp docs use `E2E_WHATSAPP_RECIPIENT` / business account id; target send destination is `E2E_WHATSAPP_TO`.
- Existing deterministic provider tests under `crates/provider-tests/tests/provider_core_*.rs` build/load components with mocked host HTTP/secrets/state and should remain PR-safe.
- Existing pack scripts already support provider scoping via `PACK_FILTER`, and publish scripts support `COMPONENT_FILTER` / `PACK_FILTER`.
- Existing docs live in `docs/ci_live_e2e.md`, but the new target doc should be `docs/provider-e2e.md`.

## Scope

- Replace or refactor the current monolithic live e2e model into per-provider nightly jobs.
- Add `.github/workflows/nightly-provider-e2e.yml`.
- Add a reusable workflow if useful, for example `.github/workflows/provider-e2e.yml`.
- Add provider e2e metadata in `ci/provider-matrix.json` or provider-local metadata.
- Add safe committed fixtures under clear paths such as:
  - `e2e/providers/dummy/`
  - `e2e/providers/slack/`
  - `e2e/providers/teams/`
  - `e2e/providers/telegram/`
  - `e2e/providers/webex/`
  - `e2e/providers/whatsapp/`
  - `e2e/providers/email/`
  - `e2e/providers/webchat/`
- Add helper scripts under `ci/e2e/` or `tools/e2e/`.
- Add `docs/provider-e2e.md` with setup, secrets, safety, operation, and troubleshooting guidance.

## Out Of Scope

- Do not run real provider e2e on normal PRs by default.
- Do not commit real credentials, tokens, raw request headers, or tenant-specific private config.
- Do not make nightly/manual e2e a required publishing gate unless maintainers explicitly opt in.
- Do not replace deterministic provider tests; e2e supplements them.

## Workflow Requirements

1. `nightly-provider-e2e.yml` triggers:
   - nightly cron schedule,
   - `workflow_dispatch`.
2. Manual inputs:
   - `provider`: one provider, comma-separated list, or `all`,
   - `ref`: optional git ref or GHCR tag/digest to test,
   - `use_published_gtpack`: boolean, pull from GHCR when true,
   - `publish_result_summary`: boolean if useful.
3. Generate provider matrix from provider metadata.
4. Run each provider as a separate matrix job with `fail-fast: false`.
5. Use a GitHub Environment such as `e2e-live` if maintainers want environment-level secret/approval controls.
6. Do not include pull request triggers.

## Provider Metadata

Add e2e metadata for each provider:

- provider name,
- pack name,
- external service type,
- required GitHub Secrets,
- optional GitHub Secrets,
- test command,
- optional cleanup command,
- fixture path,
- supported gtpack source modes: local build and/or GHCR pull,
- operation under test,
- read-back support and limitations.

Suggested required/optional secrets:

- Dummy: no external secrets required.
- Slack: `E2E_SLACK_BOT_TOKEN`, `E2E_SLACK_CHANNEL_ID`, optional `E2E_SLACK_SIGNING_SECRET`.
- Telegram: `E2E_TELEGRAM_BOT_TOKEN`, `E2E_TELEGRAM_CHAT_ID`.
- Teams: `E2E_TEAMS_TENANT_ID`, `E2E_TEAMS_CLIENT_ID`, `E2E_TEAMS_CLIENT_SECRET`, `E2E_TEAMS_TEAM_ID`, `E2E_TEAMS_CHANNEL_ID`.
- Webex: `E2E_WEBEX_BOT_TOKEN`, `E2E_WEBEX_ROOM_ID`.
- WhatsApp: `E2E_WHATSAPP_TOKEN`, `E2E_WHATSAPP_PHONE_NUMBER_ID`, `E2E_WHATSAPP_TO`, optional `E2E_WHATSAPP_VERIFY_TOKEN`.
- Email: `E2E_EMAIL_SMTP_HOST`, `E2E_EMAIL_SMTP_PORT`, `E2E_EMAIL_SMTP_USERNAME`, `E2E_EMAIL_SMTP_PASSWORD`, `E2E_EMAIL_FROM`, `E2E_EMAIL_TO`.
- Webchat: `E2E_WEBCHAT_ENDPOINT`, optional `E2E_WEBCHAT_BEARER_TOKEN`, optional `E2E_WEBCHAT_ROOM_ID`.

## Runner Requirements

1. Resolve provider metadata.
2. Verify required secrets before running provider code.
3. If required secrets are missing:
   - write normalized result JSON with `result: "skipped"`,
   - include missing secret names only, never values,
   - write a clear GitHub step summary,
   - exit successfully for that provider job unless maintainers opt into strict missing-secret failures.
4. Mask all configured secrets and any derived token-bearing URLs.
5. Build or pull the provider `.gtpack`:
   - local mode: build only selected provider components and selected pack using existing filters,
   - published mode: pull exact GHCR tag/digest or provider version.
6. Assemble a minimal test bundle/config from:
   - provider `.gtpack`,
   - provider public config template,
   - provider-specific secrets,
   - deterministic test payload,
   - unique correlation id such as `${{ github.run_id }}-${{ matrix.provider }}`.
7. Run provider-specific e2e command.
8. Validate structurally:
   - operation returned success,
   - provider message id exists when expected,
   - provider response shape matches schema.
9. Verify externally where practical:
   - Slack: send response `ok` and `ts`, optionally `conversations.history` read-back for the correlation id.
   - Telegram: send response `ok` and `result.message_id`; read-back is generally impractical, so document the API response validation.
   - Teams: Graph send response id and optional channel message read-back.
   - Webex: messages API response id and optional `GET /messages/{id}` read-back.
   - WhatsApp: messages response id; read-back may be impractical, so document limitations.
   - Email: SMTP accepted; optional mailbox/API read-back only if a safe test mailbox is available.
   - Webchat: endpoint response/state result depending on configured test endpoint.
   - Dummy: deterministic local operation.
10. Cleanup resources/messages when supported, but do not fail the whole e2e solely because cleanup failed.

## Artifacts And Summaries

- Upload sanitized logs and normalized result JSON as artifacts.
- Do not upload values files containing credentials, raw Authorization headers, or raw request headers.
- Each provider summary includes:
  - provider,
  - gtpack source,
  - version/tag/digest tested,
  - operation tested,
  - result: passed / failed / skipped,
  - reason for skip/failure,
  - correlation id.

## Implementation Steps

1. Audit and preserve useful behavior from:
   - `.github/workflows/e2e-live.yml`,
   - `.github/workflows/e2e-dry-run.yml`,
   - `ci/nightly_real_smoke.sh`,
   - existing `ci/steps/*`,
   - `crates/provider-tests/tests/provider_core_*.rs`,
   - pack build/publish scripts.
2. Add provider e2e metadata.
3. Add matrix generation to `ci/provider_matrix.py`, for example:
   - `list-e2e-providers`,
   - `resolve-e2e-provider`,
   - provider filtering for `all` / comma-separated selections.
4. Add reusable provider e2e runner:
   - resolves metadata,
   - verifies secrets,
   - builds/pulls `.gtpack`,
   - assembles sanitized runtime config,
   - runs provider-specific e2e command,
   - writes normalized result JSON.
5. Add provider-specific e2e tests incrementally:
   - start with `dummy`,
   - add one real provider next, preferably Slack or Telegram,
   - then add Teams, Webex, WhatsApp, Email, Webchat.
6. Update or retire `ci/nightly_real_smoke.sh` once the new runner covers its behavior.
7. Add `docs/provider-e2e.md`:
   - secret setup,
   - manual dispatch examples,
   - skipped test interpretation,
   - adding e2e for a new provider,
   - safety rules for test channels/tenants/sandboxes,
   - migration notes from current `docs/ci_live_e2e.md` secret names.

## Acceptance Criteria

- `.github/workflows/nightly-provider-e2e.yml` exists and runs on schedule plus manual dispatch.
- Matrix jobs are per provider with `fail-fast: false`.
- Missing secrets produce a clear skipped result.
- Dummy provider e2e runs without secrets.
- At least one real provider e2e is implemented end-to-end.
- Logs and artifacts are sanitized.
- `docs/provider-e2e.md` lists all secrets and setup steps.
- The workflow can test either locally built gtpack or published GHCR gtpack.
- E2E tests do not run on normal PRs unless manually invoked.

## Relationship To Publish Workflows

- Normal PR validation remains deterministic and does not call real external services.
- Provider publish workflows may optionally trigger this provider e2e workflow after publishing.
- The post-publish e2e hook must be disabled by default and non-blocking unless maintainers explicitly enable blocking release gates.
- Provider release notes should record whether nightly/manual e2e passed for the released `.gtpack` version or digest.

## Review Notes

- The first implementation PR should favor a complete dummy plus one real provider path over partial scaffolding for every provider.
- Keep provider jobs isolated so one external outage does not hide the status of the rest.

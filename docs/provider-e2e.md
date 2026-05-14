# Provider Nightly E2E

`Nightly Provider E2E` runs provider jobs independently on a nightly schedule
and through manual dispatch. It does not run on normal PRs because it can use
real external services and GitHub Secrets.

## Manual Dispatch

Inputs:

- `provider`: one provider, comma-separated providers, or `all`.
- `ref`: optional git ref for local-build mode, or GHCR tag/digest for
  `use_published_gtpack=true`.
- `use_published_gtpack`: pull the `.gtpack` from GHCR instead of building it
  locally.
- `publish_result_summary`: reserved for summary behavior; result JSON artifacts
  are always uploaded.

Examples:

```text
provider=slack
use_published_gtpack=false
```

```text
provider=slack,telegram
ref=0.5.0
use_published_gtpack=true
```

## Secrets

Use provider-scoped GitHub Secrets. Missing required secrets mark that provider
job as `skipped` with a clear reason; secret values are never printed.

| Provider | Required secrets | Optional secrets |
| --- | --- | --- |
| Dummy | none | none |
| Slack | `E2E_SLACK_BOT_TOKEN`, `E2E_SLACK_CHANNEL_ID` | `E2E_SLACK_SIGNING_SECRET` |
| Telegram | `E2E_TELEGRAM_BOT_TOKEN`, `E2E_TELEGRAM_CHAT_ID` | none |
| Teams | `E2E_TEAMS_TENANT_ID`, `E2E_TEAMS_CLIENT_ID`, `E2E_TEAMS_CLIENT_SECRET`, `E2E_TEAMS_TEAM_ID`, `E2E_TEAMS_CHANNEL_ID` | none |
| Webex | `E2E_WEBEX_BOT_TOKEN`, `E2E_WEBEX_ROOM_ID` | none |
| WhatsApp | `E2E_WHATSAPP_TOKEN`, `E2E_WHATSAPP_PHONE_NUMBER_ID`, `E2E_WHATSAPP_TO` | `E2E_WHATSAPP_VERIFY_TOKEN` |
| Email | `E2E_EMAIL_SMTP_HOST`, `E2E_EMAIL_SMTP_PORT`, `E2E_EMAIL_SMTP_USERNAME`, `E2E_EMAIL_SMTP_PASSWORD`, `E2E_EMAIL_FROM`, `E2E_EMAIL_TO` | none |
| Webchat | `E2E_WEBCHAT_ENDPOINT` | `E2E_WEBCHAT_BEARER_TOKEN`, `E2E_WEBCHAT_ROOM_ID` |

Legacy names from the previous live workflow should be migrated:

- `E2E_SLACK_CHANNEL` -> `E2E_SLACK_CHANNEL_ID`
- `E2E_SMTP_*` -> `E2E_EMAIL_SMTP_*`
- `E2E_MS_*` -> `E2E_TEAMS_*`
- `E2E_WHATSAPP_RECIPIENT` -> `E2E_WHATSAPP_TO`

## Implemented Operations

- Dummy: validates that the selected `.gtpack` exists and records a deterministic
  local result.
- Slack: sends a low-impact `chat.postMessage` to the configured test channel
  and validates `ok` plus `ts`. It attempts a `conversations.history` read-back
  for the unique correlation id when the bot has permission.

Other providers have metadata and safe fixtures in place. Their jobs currently
skip with a clear “operation not implemented” result until provider-specific
live operations are added.

## Safety

- Use dedicated test channels, rooms, chats, phone numbers, tenants, and
  mailboxes.
- Do not point e2e secrets at production destinations.
- Do not put secrets in fixture files or HTML.
- Use HTTPS for external endpoints.
- Cleanup should be best-effort; cleanup failure should not hide the primary
  send result.

## Artifacts

Each matrix job uploads `provider-e2e-<provider>` containing sanitized
`<provider>.json` with:

- provider
- gtpack source
- version
- operation
- result: `passed`, `failed`, or `skipped`
- skip/failure reason
- correlation id
- provider message id when available

Raw request headers, Authorization values, and credential-bearing URLs must not
be uploaded.

## Adding A Provider E2E

1. Add or update the provider `e2e` block in `ci/provider-matrix.json`.
2. Add a safe fixture under `e2e/providers/<provider>/`.
3. Add a provider runner in `ci/e2e/provider_e2e.py`.
4. Document required secrets above.
5. Run manually with `provider=<provider>` before enabling broader schedules.

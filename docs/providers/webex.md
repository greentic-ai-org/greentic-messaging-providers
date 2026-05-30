# Webex Provider

## What It Does

Webex connects Greentic to Cisco Webex rooms and people through the Webex REST API.

## Features

- Sends messages to rooms, person IDs, or person email addresses.
- Supports replies through Webex parent IDs when available.
- Handles Webex webhooks.
- Fetches full message details when a webhook only includes an event reference.
- Sends Adaptive Cards as Webex attachments with fallback text.
- Supports nightly e2e against a dedicated Webex room.

## Setup Inputs

Common setup values include:

- Public base URL for callbacks.
- Room ID for the Webex room the bot can read and post to.
- Optional person email destination for direct messages.
- Webex bot token secret.
- Optional API base URL for non-default environments.
- Webhook secret for Webex `X-Spark-Signature` verification.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `WEBEX_BOT_TOKEN` | Yes | Webex bot access token used for Messages API calls. |
| `WEBEX_WEBHOOK_SECRET` | Yes | Shared secret used to verify Webex webhook signatures. |

Nightly e2e uses `E2E_WEBEX_BOT_TOKEN` and `E2E_WEBEX_ROOM_ID`.

## Message Features

- Outbound: Webex Messages API.
- Inbound: Webex webhook events plus optional message read-back.
- Replies: supported when parent/thread metadata is available.
- Adaptive Cards: sent as Webex attachments with fallback text.
- External read-back: optional GET `/messages/{id}` where configured.

## Owned Files

- `components/webex/`
- `components/webex-webhook/`
- `components/messaging-provider-webex/`
- `packs/messaging-webex/`
- `crates/provider-tests/tests/provider_core_webex.rs`
- `e2e/providers/webex/`

## Focused Checks

```bash
cargo test -p messaging-provider-webex
cargo test -p webex-webhook
cargo test -p provider-tests provider_core_webex
PACK_FILTER=messaging-webex ./ci/steps/11_build_packs.sh
```

## Agent Notes

Webex webhooks often require a second API call to fetch message text. Preserve that distinction when editing ingest behavior or writing tests.

# Telegram Provider

## What It Does

Telegram connects Greentic to Telegram chats through the Telegram Bot API.

## Features

- Sends bot messages with `sendMessage`.
- Supports default chat configuration.
- Handles Telegram update webhooks.
- Includes a webhook reconciliation component for registering or checking webhook URLs.
- Converts rich content to Telegram-friendly text/HTML.
- Supports nightly e2e against a dedicated Telegram chat.

## Setup Inputs

Common setup values include:

- Public base URL for webhook callbacks.
- Default chat ID.
- Bot token secret.
- Optional webhook path or secret token.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `TELEGRAM_BOT_TOKEN` | Yes | Telegram bot token used for send and webhook operations. |

Nightly e2e uses `E2E_TELEGRAM_BOT_TOKEN` and `E2E_TELEGRAM_CHAT_ID`.

## Message Features

- Outbound: Telegram bot messages.
- Inbound: Telegram update payloads.
- Replies: supported with Telegram reply metadata.
- Adaptive Cards: converted to text/HTML suitable for Telegram.
- External read-back: send API response validation is the baseline.

## Owned Files

- `components/telegram/`
- `components/telegram-webhook/`
- `components/messaging-ingress-telegram/`
- `components/messaging-provider-telegram/`
- `packs/messaging-telegram/`
- `crates/provider-tests/tests/provider_core_telegram.rs`
- `e2e/providers/telegram/`

## Focused Checks

```bash
cargo test -p messaging-provider-telegram
cargo test -p messaging-ingress-telegram
cargo test -p telegram-webhook
cargo test -p provider-tests provider_core_telegram
PACK_FILTER=messaging-telegram ./ci/steps/11_build_packs.sh
```

## Agent Notes

Telegram has separate provider-core, ingress, and webhook reconciliation components. Be precise about which component owns a bug.


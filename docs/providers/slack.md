# Slack Provider

## What It Does

Slack connects Greentic to Slack channels through a Slack app. It can send messages and handle Slack webhook/event payloads.

## Features

- Sends messages with Slack `chat.postMessage`.
- Supports default channel configuration.
- Handles Slack ingress through the Slack ingress component.
- Can verify Slack webhook signatures when a signing secret is configured.
- Includes setup, validation, diagnostics, webhook verification, and credential rotation flows in the pack.
- Supports nightly e2e against a dedicated Slack test channel.

## Setup Inputs

Common setup values include:

- Public base URL for webhook callbacks.
- Default Slack channel.
- Bot token secret.
- Optional signing secret.
- Optional app/configuration token for webhook registration support.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `SLACK_BOT_TOKEN` | Yes | Bot token used for `chat.postMessage`. |
| `SLACK_SIGNING_SECRET` | Optional, recommended for ingress | Verifies Slack webhook signatures. |
| `SLACK_APP_ID` | Optional | Supports manifest or webhook registration flows. |
| `SLACK_CONFIGURATION_TOKEN` | Optional | Supports Slack app configuration updates. |

Nightly e2e uses `E2E_SLACK_BOT_TOKEN` and `E2E_SLACK_CHANNEL_ID`.

## Message Features

- Outbound: channel messages.
- Inbound: Slack webhook/event payloads.
- Replies: supported where Slack thread metadata is available.
- Adaptive Cards: downsampled to Slack-friendly text and simple actions.
- External read-back: nightly e2e can attempt `conversations.history` when permissions allow it.

## Owned Files

- `components/slack/`
- `components/messaging-ingress-slack/`
- `components/messaging-provider-slack/`
- `packs/messaging-slack/`
- `crates/provider-tests/tests/provider_core_slack.rs`
- `e2e/providers/slack/`

## Focused Checks

```bash
cargo test -p messaging-provider-slack
cargo test -p messaging-ingress-slack
cargo test -p provider-tests provider_core_slack
PACK_FILTER=messaging-slack ./ci/steps/11_build_packs.sh
```

## Agent Notes

Slack has both provider-core and ingress components. Keep send-path changes in `messaging-provider-slack` and webhook parsing changes in `messaging-ingress-slack` unless shared behavior is intended.


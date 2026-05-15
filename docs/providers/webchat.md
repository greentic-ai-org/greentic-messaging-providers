# WebChat Provider

## What It Does

WebChat provides a Direct Line-style chat backend for browser or app chat experiences. It is not an external SaaS connector; it uses Greentic-hosted state and token handling.

## Features

- Provides Direct Line-compatible conversation and activity operations.
- Stores conversation state through the host state store.
- Signs and verifies Direct Line JWT tokens.
- Sends bot messages into WebChat conversations.
- Ingests browser/user activities.
- Supports native Adaptive Card payloads.

## Setup Inputs

Common setup values include:

- Public base URL.
- Route/channel name.
- Tenant channel ID.
- Delivery mode, such as `local_queue`, `websocket`, or `pubsub`.
- JWT signing key secret.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `jwt_signing_key` | Yes | Signs and verifies Direct Line tokens. |
| `WEBCHAT_TOKEN` | Optional | Used by some validation and conformance checks. |

Nightly e2e uses `E2E_WEBCHAT_ENDPOINT` and optional bearer/room secrets.

## Message Features

- Outbound: writes bot activities to WebChat conversation state.
- Inbound: receives Direct Line/browser activities.
- Replies: conversation-based rather than external thread-based.
- Adaptive Cards: native WebChat support.
- External read-back: endpoint response validation or local state validation.

## Owned Files

- `components/webchat/`
- `components/messaging-provider-webchat/`
- `crates/webchat-directline-core/`
- `packs/messaging-webchat/`
- `crates/provider-tests/tests/provider_core_webchat.rs`
- `e2e/providers/webchat/`

## Focused Checks

```bash
cargo test -p messaging-provider-webchat
cargo test -p webchat-directline-core
cargo test -p provider-tests provider_core_webchat
PACK_FILTER=messaging-webchat ./ci/steps/11_build_packs.sh
```

## Agent Notes

WebChat uses state-store behavior. Avoid treating it like Slack, Telegram, or Webex, which call external HTTP APIs for delivery.


# Provider Catalog

This catalog summarizes every messaging provider in this repository. Each provider has its own feature sheet with setup notes, secrets, supported message behavior, tests, and code paths.

## Quick Comparison

| Provider | Best For | Sends Messages | Receives Messages | Adaptive Cards | External Credentials |
| --- | --- | --- | --- | --- | --- |
| [Dummy](dummy.md) | Local tests and examples | Yes, simulated | No | Test-focused | Optional dummy tokens |
| [Email](email.md) | SMTP-style outbound email | Yes | No | HTML/text fallback | SMTP password |
| [Slack](slack.md) | Slack channels and app messages | Yes | Yes, events/webhooks | Text fallback | Slack bot token, optional signing/config secrets |
| [Teams](teams.md) | Microsoft Teams bot and Graph delivery | Yes | Yes, Bot Framework webhooks | Native Adaptive Cards | Azure Bot and Graph secrets |
| [Telegram](telegram.md) | Telegram bot chats | Yes | Yes, Telegram updates | Text fallback | Telegram bot token |
| [WebChat](webchat.md) | Direct Line-style web chat backend | Yes | Yes, Direct Line/local state | Native Adaptive Cards | JWT signing key |
| [WebChat GUI](webchat-gui.md) | Hosted or embedded browser chat UI | Yes, via WebChat backend | Yes, via browser Direct Line flow | Native Adaptive Cards | JWT signing key |
| [Webex](webex.md) | Cisco Webex rooms and people | Yes | Yes, webhooks with message lookup | Attachment plus fallback text | Webex bot token |
| [WhatsApp](whatsapp.md) | WhatsApp Business Cloud API | Yes | Yes, Cloud API webhooks | Text/media fallback | Cloud API token, optional verify token |

## Common Features

All production providers share these capabilities unless their feature sheet says otherwise:

- Interactive setup through `gtc setup`.
- Provider configuration schema under `packs/messaging-<provider>/schemas/`.
- Provider pack output as `dist/packs/messaging-<provider>.gtpack`.
- Standard provider operations for setup, validation, rendering, encoding, and sending.
- Local tests under `crates/provider-tests/tests/`.
- Dependency-aware CI routing through `ci/provider-matrix.json`.

## How To Use These Files

For non-technical developers:

1. Open the provider feature sheet.
2. Read "What It Does" and "Setup Inputs".
3. Give the listed secret names to whoever manages GitHub or Greentic secrets.
4. Use the testing section to confirm the provider works.

For coding agents:

1. Read the provider feature sheet.
2. Use "Owned Files" before editing.
3. Use "Focused Checks" before broad test runs.
4. Keep changes within provider-owned paths unless the task explicitly touches shared code.


# Microsoft Email Provider

## What It Does

Microsoft Email sends outbound email through Microsoft Graph. It is the Microsoft-specific email provider and owns Graph OAuth, Graph sendMail payloads, webhook ingress, and subscription operations.

Use this provider instead of `messaging-email` for Microsoft 365 mailboxes. The generic `messaging-email` pack is the legacy SMTP contract.

## Provider Identity

- Pack ID: `messaging-microsoft-email`
- Provider type: `messaging.email.microsoft_graph`
- Component: `messaging-provider-email`

## Setup Inputs

Common setup values include:

- From address.
- Microsoft Entra tenant ID.
- Microsoft Graph OAuth client ID.
- Microsoft Graph OAuth refresh token.
- Microsoft Graph OAuth client secret.
- Public base URL when webhook ingress or subscriptions are enabled.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `FROM_ADDRESS` | Yes | Sender mailbox. |
| `GRAPH_TENANT_ID` | Yes | Microsoft Entra tenant ID. |
| `MS_GRAPH_CLIENT_ID` | Yes | OAuth client/application ID. |
| `MS_GRAPH_REFRESH_TOKEN` | Yes for delegated send | OAuth refresh token. |
| `MS_GRAPH_CLIENT_SECRET` | Yes | OAuth client secret. |

## Message Features

- Outbound: Graph `sendMail`.
- Inbound: Graph webhook normalization through `ingest_http`.
- Subscriptions: Graph subscription operations are exposed by the provider component. Startup subscription management should use provider-declared subscription metadata when that contract is enabled.
- Adaptive Cards: converted to email-friendly HTML.
- Attachments: reference attachments are supported where a URL is available.

## Focused Checks

```bash
python3 tools/provider_build_answers.py --check microsoft-email
cargo test -p messaging-provider-email
cargo test -p provider-tests --test provider_build_answers
scripts/build_providers.sh microsoft-email
```

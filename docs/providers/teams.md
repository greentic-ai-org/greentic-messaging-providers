# Microsoft Teams Provider

## What It Does

Teams connects Greentic to Microsoft Teams through Microsoft Graph. No Azure Bot Service is required for the default provider path.

## Features

- Sends channel messages, channel replies, and chat messages through Microsoft Graph.
- Uses delegated Microsoft OAuth tokens for Graph access.
- Supports Graph subscription lifecycle and Graph notification ingress.
- Supports Microsoft OAuth device-code setup and validation for Graph credentials.
- Includes diagnostics and subscription flows in the pack.

## Setup Inputs

Common setup values include:

- Microsoft app/client ID, only when it is not supplied by hosted config/env.
- Microsoft tenant ID discovered after device-code login.
- Graph refresh token from device-code login.
- Optional test-only access token.
- Default team/channel or chat destination.
- Public base URL only when enabling Graph change-notification ingress.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `MS_GRAPH_TENANT_ID` | Yes | Microsoft Entra tenant ID. |
| `MS_GRAPH_CLIENT_ID` | Yes | Microsoft Entra app client ID. |
| `MS_GRAPH_REFRESH_TOKEN` | Yes for normal runtime sends | Delegated Graph refresh token. |
| `MS_GRAPH_ACCESS_TOKEN` | Optional/test-only | Direct Graph access-token override for local testing. |

Nightly e2e uses `E2E_TEAMS_` GitHub Secrets. See [Provider nightly e2e](../provider-e2e.md).

## Message Features

- Outbound: Microsoft Graph `chatMessage` APIs.
- Inbound: Microsoft Graph change notifications.
- Replies: channel replies through Graph message replies.
- Adaptive Cards: fallback-rendered as text/html until a dedicated Teams card strategy is added.

## Setup Modes

1. Hosted/SaaS: click Connect Microsoft Teams; Greentic supplies the client ID and runs device-code setup.
2. Self-hosted: create a multi-tenant Entra app registration with public client/device-code flow enabled, then login.
3. Manual/test: paste `tenant_id`, `client_id`, `refresh_token`, or test-only `access_token`.

Default setup uses the Microsoft `organizations` device-code endpoints and does not need a redirect URL or client secret.

Default delegated scopes:

```text
offline_access openid profile User.Read Team.ReadBasic.All Channel.ReadBasic.All ChannelMessage.Send ChannelMessage.Read.All Chat.Read
```

## Limitations

- This is not a native Bot Framework bot.
- It does not handle Bot Framework command routing by default.
- Messages are sent as the Graph-authorized identity according to Microsoft Graph permissions.
- Graph notification webhooks require a public HTTPS callback URL.
- Teams message subscriptions require Graph read consent such as `ChannelMessage.Read.All`.

## Owned Files

- `components/teams/`
- `components/messaging-ingress-teams/`
- `components/messaging-provider-teams/`
- `packs/messaging-teams/`
- `e2e/providers/teams/`

## Focused Checks

```bash
cargo test -p messaging-provider-teams
cargo test -p messaging-ingress-teams
scripts/build_providers.sh teams
```

## Agent Notes

Keep Graph credential names explicit and never log tokens. Azure Bot Service credentials are legacy-only and not part of the default path.

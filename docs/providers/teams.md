# Microsoft Teams Provider

## What It Does

Teams connects Greentic to Microsoft Teams through a Bot Framework-compatible Teams app setup path, with Microsoft Graph used for setup-time app registration, Teams app catalog publish/install, and existing Graph-based provider operations.

## Features

- Registers a Bot Framework-compatible Teams endpoint for inbound Teams activity.
- Publishes and installs the Teams app through Microsoft Graph.
- Uses delegated Microsoft OAuth tokens for Graph setup actions.
- Uses Azure management OAuth for Microsoft Bot/Teams channel registration.
- Supports Graph subscription lifecycle and Graph notification ingress.
- Supports Microsoft OAuth device-code setup and validation for Graph and Azure management credentials.
- Includes diagnostics and subscription flows in the pack.

## Setup Inputs

Common setup values include:

- Public base URL for the active Greentic runtime/tunnel.
- Bot display name.
- Optional existing Bot app ID and Bot app password; setup can create these through Graph when not supplied.
- Optional Azure subscription/resource group/location values for Bot channel registration.
- Microsoft tenant/client values when they are not supplied by hosted config/env.

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

Default setup uses Microsoft `organizations` device-code endpoints and does not need a redirect URL.

Default Graph delegated setup scopes:

```text
offline_access User.Read Application.ReadWrite.All AppCatalog.ReadWrite.All TeamsAppInstallation.ReadWriteForUser
```

Default Azure management setup scope:

```text
offline_access https://management.azure.com/user_impersonation
```

## Limitations

- Setup requires Microsoft Bot/Teams channel registration for the Bot Framework-compatible endpoint.
- Graph notification webhooks require a public HTTPS callback URL.
- Teams message subscriptions require Graph read consent such as `ChannelMessage.Read.All`.

## Owned Files

- `components/teams/`
- `components/messaging-ingress-teams/`
- `components/messaging-provider-teams-graph/`
- `packs/messaging-teams-graph/`
- `e2e/providers/teams/`

## Focused Checks

```bash
cargo test -p messaging-provider-teams-graph
cargo test -p messaging-ingress-teams
scripts/build_providers.sh teams
```

## Agent Notes

Keep Graph, Azure management, and Bot Framework credential names explicit and never log tokens.

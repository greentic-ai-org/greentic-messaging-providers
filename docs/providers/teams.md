# Microsoft Teams Provider

## What It Does

Teams connects Greentic to Microsoft Teams through Azure Bot Service and Microsoft Graph/Bot Framework APIs.

## Features

- Sends messages to Teams channels or conversations.
- Supports Microsoft Bot Framework ingress.
- Supports native Adaptive Cards for Teams.
- Supports setup and validation for Azure/Graph credentials.
- Includes subscription and diagnostics flows in the pack.
- Supports nightly e2e metadata for dedicated Teams test tenants/channels.

## Setup Inputs

Common setup values include:

- Public base URL for callbacks.
- Azure tenant ID.
- Microsoft app/client ID.
- Bot app ID.
- Bot app password secret.
- Graph client secret or refresh token, depending on the selected mode.
- Default team/channel or conversation destination.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `MS_BOT_APP_ID` | Yes for Bot Framework | Azure Bot app ID. |
| `MS_BOT_APP_PASSWORD` | Yes for Bot Framework | Azure Bot app password/client secret. |
| `MS_GRAPH_TENANT_ID` | Often required | Azure AD/Entra tenant ID. |
| `MS_GRAPH_CLIENT_ID` | Often required | Graph app client ID. |
| `MS_GRAPH_CLIENT_SECRET` | Often required | Graph token acquisition. |
| `MS_GRAPH_REFRESH_TOKEN` | Optional by mode | Refresh-token based Graph access. |

Nightly e2e uses `E2E_TEAMS_` GitHub Secrets. See [Provider nightly e2e](../provider-e2e.md).

## Message Features

- Outbound: Teams channel or conversation send.
- Inbound: Bot Framework activity webhooks.
- Replies: supported when service URL and conversation/thread metadata are available.
- Adaptive Cards: native Teams Adaptive Card support.
- External read-back: e2e can validate Graph response IDs when enabled.

## Owned Files

- `components/teams/`
- `components/messaging-ingress-teams/`
- `components/messaging-provider-teams/`
- `packs/messaging-teams/`
- `crates/provider-tests/tests/provider_core_teams.rs`
- `e2e/providers/teams/`

## Focused Checks

```bash
cargo test -p messaging-provider-teams
cargo test -p messaging-ingress-teams
cargo test -p provider-tests provider_core_teams
PACK_FILTER=messaging-teams ./ci/steps/11_build_packs.sh
```

## Agent Notes

Teams auth is easy to break accidentally. Keep credential naming explicit, never log tokens, and preserve Bot Framework and Graph distinctions in docs and config.


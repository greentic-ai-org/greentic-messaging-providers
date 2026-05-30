# Messaging Provider Teams Component

Provider-core Microsoft Teams messaging provider. Outbound messaging uses
Microsoft Graph. Bot Framework fields are retained for inbound Teams bot
activities.

## Component ID
- `messaging-provider-teams`

## Provider types
- `messaging.teams.graph`
- `messaging.teams.bot` is accepted by `send_payload` for compatibility, but
  outbound delivery still resolves a Graph destination.

## Secrets

All secrets are stored under the URI prefix
`secrets://{env}/{tenant}/_/messaging-teams/` where `{env}` must match
`GREENTIC_ENV` (e.g. `dev`).

| Key | Required | Description |
|-----|----------|-------------|
| `MS_GRAPH_TENANT_ID` | Yes | Azure AD tenant ID |
| `MS_GRAPH_CLIENT_ID` | Yes | Azure AD application (client) ID |
| `MS_GRAPH_REFRESH_TOKEN` | Yes for normal sends | Refresh token from delegated Microsoft device-code setup |
| `MS_GRAPH_ACCESS_TOKEN` | Test only | Direct access-token override for local testing |

## Setup modes

### `graph_channel`

The default setup path uses Microsoft OAuth device-code login against the
`organizations` endpoint. It does not require a redirect URL, Azure Bot
Service, Bot Framework, or a client secret.

Graph channel setup persists machine IDs and display labels:

| Key | Purpose |
|-----|---------|
| `team_id` | Authoritative Microsoft Graph Team ID used for routing |
| `team_name` | Human-readable selected Team display name |
| `channel_id` | Authoritative Microsoft Graph Channel ID used for routing |
| `channel_name` | Human-readable selected Channel display name |
| `desired_channel_name` | Desired standard channel name, often seeded from the bundle name |

During `apply_answers`, the provider lists existing channels in the selected
Team, reuses an exact case-insensitive display-name match, and otherwise
creates a standard channel with Microsoft Graph `POST /teams/{team_id}/channels`.
The resulting `channel_id` and `channel_name` are returned in the setup config.

Only `team_id` and `channel_id` are used to build Graph message URLs. Names are
stored for diagnostics, setup summaries, and user-facing display; they are not
unique and must not be used as routing identifiers.

### `bot_framework`

Bot Framework mode is for inbound Teams bot activities and Teams app manifest
configuration. It accepts:

| Key | Purpose |
|-----|---------|
| `ms_bot_app_id` | Azure Bot app ID |
| `ms_bot_app_password` | Azure Bot app password |
| `bot_display_name` | Human-readable Teams bot name |
| `messaging_endpoint` | Public Bot Framework messaging endpoint |

This mode does not remove the existing `ingest_http` Bot Framework behavior.
It also does not make Graph sends work without Graph credentials and an
authoritative channel or chat ID.

### Obtaining a refresh token

1. Register an app in Azure AD with **Delegated** permissions:
   `offline_access`, `openid`, `profile`, `User.Read`, `Team.ReadBasic.All`,
   `Channel.ReadBasic.All`, `Channel.Create`, `ChannelMessage.Send`,
   `ChannelMessage.Read.All`, and `Chat.Read`.
2. Enable **public client/device-code flow**.
3. Run `gtc setup` once generic device-code setup is available, or use
   `scripts/test_teams.sh` for local validation.
4. Save the returned `refresh_token` as `MS_GRAPH_REFRESH_TOKEN` and the app
   client ID as `MS_GRAPH_CLIENT_ID`.

## Destination formats

The `--to` flag (or `to[0].id` in the envelope) accepts two formats:

| Format | Kind | Example |
|--------|------|---------|
| `{team_id}:{channel_id}` | `channel` | `c3392cbc-2cb0-48e8-9247-504d8defea40:19:abc...@thread.tacv2` |
| `{chat_id}` | `chat` | `19:meeting_abc...@thread.v2` |

The provider auto-detects the kind based on whether the ID contains a `:` separator.

## Quick start

### Send a text message to a channel

```bash
GREENTIC_ENV=dev gtc op demo send \
  --bundle demo-bundle \
  --provider messaging-teams \
  --to "{team_id}:{channel_id}" \
  --text "Hello from Greentic" \
  --tenant demo --env dev
```

### Test ingress (CLI)

```bash
GREENTIC_ENV=dev gtc op demo ingress \
  --bundle demo-bundle \
  --provider messaging-teams \
  --tenant demo \
  --body /tmp/teams-webhook.json
```

### Test ingress (operator HTTP)

```bash
# Start operator
GREENTIC_ENV=dev gtc op demo start \
  --bundle demo-bundle --cloudflared off --nats off \
  --skip-setup --skip-secrets-init --domains messaging

# POST webhook
curl -X POST http://localhost:8080/messaging/ingress/messaging-teams/demo/default \
  -H "Content-Type: application/json" \
  -d @/tmp/teams-webhook.json
```

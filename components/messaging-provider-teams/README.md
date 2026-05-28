# Messaging Provider Teams Component

Provider-core Microsoft Teams messaging provider.

## Component ID
- `messaging-provider-teams`

## Provider types
- `messaging.teams.graph`

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

### Device-code setup

The default setup path uses Microsoft OAuth device-code login against the
`organizations` endpoint. It does not require a redirect URL, Azure Bot
Service, Bot Framework, or a client secret.

### Obtaining a refresh token

1. Register an app in Azure AD with **Delegated** permissions:
   `offline_access`, `openid`, `profile`, `User.Read`, `Team.ReadBasic.All`,
   `Channel.ReadBasic.All`, `ChannelMessage.Send`, `ChannelMessage.Read.All`,
   and `Chat.Read`.
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

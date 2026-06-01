# PR 4: Add Interactive Teams Graph Tester

## Review Status

Reviewed against the current codebase on 2026-05-27 and adapted.

This PR should come after PRs 1-3. `scripts/test_slack.sh` is a good reference, but Teams should not copy Slack's manifest-creation flow directly. It should focus on Microsoft OAuth, Graph discovery, provider-path send/reply, subscription lifecycle, and webhook validation/logging.

## Title

Add interactive Teams Graph tester script

## Goal

Add `scripts/test_teams.sh`, similar in spirit to `scripts/test_slack.sh`, for local end-to-end Graph-first Teams validation:

- cloudflared public URL
- Microsoft OAuth login/token exchange
- Graph team/channel/chat discovery
- provider-path channel send, reply, and chat send
- Graph subscription creation/renew/delete where supported
- webhook validation-token response
- incoming notification display

No Azure Bot Service, `MS_BOT_APP_ID`, `MS_BOT_APP_PASSWORD`, or Bot Framework setup.

## Ordering Constraint

Implement after:

1. PR 1 gives the provider a working Graph egress path.
2. PR 2 updates setup/schema fields to Graph names.
3. PR 3 makes Graph ingress/subscriptions real enough for tester hooks.

Before those PRs land, this script can only be a partial Graph API tester and would not validate the provider path accurately.

## Script Shape

Add:

```text
scripts/test_teams.sh [--port <port>] [--no-build] [--no-open]
```

Default port:

```text
8792
```

Build steps:

- `scripts/build_providers.sh teams` unless `--no-build`
- `cargo build -p greentic-messaging-tester`

Runtime pattern should mirror Slack tester:

- create a tmp work dir
- write embedded `server.py`
- start local UI
- start cloudflared
- persist `teams-values.json`
- append event log JSONL
- open browser unless `--no-open`

## Environment

Support:

- `PORT`
- `CLOUDFLARED_BIN`
- `GREENTIC_TEAMS_CLIENT_ID`
- `GREENTIC_TEAMS_TENANT_ID`
- `GREENTIC_TEAMS_CLIENT_SECRET`
- `GREENTIC_TEAMS_SCOPES`

Never print raw access or refresh tokens in shell logs.

## UI Sections

1. Public URL
   - cloudflared URL
   - Teams ingress endpoint:
     `/v1/messaging/ingress/messaging-teams/default/default`

2. OAuth setup
   - fields:
     - `client_id`
     - `tenant_id`, default `common` or `organizations`
     - scopes
     - optional `client_secret`
   - buttons:
     - Build login URL
     - Login with Microsoft
     - Exchange code
     - Refresh token
     - Save values
   - callback:
     - `/oauth/callback/teams`
   - token endpoint:
     - `https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token`

3. Discovery
   - Get me
   - List joined teams: `GET /me/joinedTeams`
   - List channels: `GET /teams/{team-id}/channels`
   - List chats only if permissions and API behavior support it.

4. Send test
   - destination kind: channel or chat
   - `team_id`
   - `channel_id`
   - `chat_id`
   - message text
   - content type: text/html
   - provider-path send via `greentic-messaging-tester`
   - optional direct Graph send button for debugging, clearly labeled as bypassing provider.

5. Reply test
   - use last channel message id as `reply_to_id`
   - provider-path reply/send_payload.

6. Graph subscriptions
   - resource auto-built from selected channel/chat
   - `changeType`, default `created`
   - expiration
   - `clientState`
   - buttons:
     - Create/ensure subscription
     - Renew subscription
     - Delete subscription if implemented
     - Simulate Graph validationToken request

7. Incoming events
   - raw webhook requests
   - normalized output from ingress path where possible
   - poll every 2 seconds.

## Values JSON

Write provider-path values in the shape expected by the new Graph config:

```json
{
  "config": {
    "provider_id": "messaging-teams",
    "tenant": "default",
    "team": "default",
    "tenant_id": "...",
    "client_id": "...",
    "public_base_url": "...",
    "graph_base_url": "https://graph.microsoft.com/v1.0",
    "auth_base_url": "https://login.microsoftonline.com",
    "team_id": "...",
    "channel_id": "...",
    "chat_id": "..."
  },
  "secrets": {
    "MS_GRAPH_REFRESH_TOKEN": "...",
    "MS_GRAPH_ACCESS_TOKEN": "...",
    "MS_GRAPH_CLIENT_SECRET": "..."
  },
  "http": "real",
  "state": {}
}
```

Omit empty optional secrets rather than storing blank strings.

## Python Server Endpoints

Implement at least:

- `GET /`
- `GET /api/state`
- `POST /api/save`
- `GET /oauth/callback/teams`
- `POST /api/token/exchange`
- `POST /api/token/refresh`
- `POST /api/discover/me`
- `POST /api/discover/teams`
- `POST /api/discover/channels`
- `POST /api/send`
- `POST /api/reply`
- `POST /api/subscriptions/create`
- `POST /api/subscriptions/renew`
- `POST /api/subscriptions/delete` if delete exists
- `GET /v1/messaging/ingress/messaging-teams/default/default`
- `POST /v1/messaging/ingress/messaging-teams/default/default`

For validation-token requests, return plain text `validationToken`.

For notification POSTs:

- append raw event to JSONL.
- call `greentic-messaging-tester` ingress path when available.
- display raw and normalized output.

## Docs

Add a short docs section for `scripts/test_teams.sh` covering:

- cloudflared prerequisite
- Entra app registration
- redirect URI matching `/oauth/callback/teams`
- delegated Graph permissions/admin consent
- no Azure Bot Service required

## Acceptance Criteria

- `scripts/test_teams.sh --help` works.
- UI starts and shows public URL.
- OAuth URL generation works.
- Token exchange and refresh work with valid app settings.
- Team/channel discovery works with valid permissions.
- Channel send works through provider path.
- Reply works through provider path.
- Subscription create/renew works if permissions and public URL are valid.
- Graph validation-token handshake works.
- Incoming webhook notifications are logged and displayed.

## Out Of Scope

- Implementing Graph egress, setup, or ingress behavior. This script should test those PRs, not secretly replace them.
- Teams app catalog publish/install.

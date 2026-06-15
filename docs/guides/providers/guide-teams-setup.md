# Microsoft Teams Graph Setup

The Teams provider is Graph-first. Azure Bot Service, Bot Framework app IDs, bot passwords, Bot Connector `serviceUrl`, and Bot Connector conversation IDs are not required for the default path.

## Architecture

```text
Greentic setup
  -> Microsoft OAuth device-code login using organizations
  -> Microsoft Graph OAuth refresh-token grant
  -> Microsoft Graph chatMessage APIs
  -> Teams channel, channel reply, or chat

Microsoft Graph change notifications
  -> public HTTPS Greentic ingress URL
  -> messaging-ingress-teams
```

## Setup Modes

### Hosted/SaaS

Use Connect Microsoft Teams. Greentic supplies the client ID, shows the Microsoft device login code, stores the derived Graph tokens as secrets, and lets the user choose a Team/Channel or Chat. No redirect URL is needed for the default setup flow.

### Self-Hosted

Create one Microsoft Entra app registration and configure delegated Graph permissions. The app registration must be multi-tenant for cross-organization users (`signInAudience = AzureADMultipleOrgs`) and public client/device-code flow must be enabled. Then login and consent with the account that should send messages.

Useful delegated permissions:

- `offline_access`
- `openid`
- `profile`
- `User.Read`
- `Team.ReadBasic.All`
- `Channel.ReadBasic.All`
- `Channel.Create`
- `ChannelMessage.Send`
- `ChannelMessage.Read.All`
- `Chat.Read`

Some tenants require admin consent for `ChannelMessage.Read.All` and other read scopes. If using the `.default` scope for runtime refresh, preconfigure the app permissions in Entra and grant consent there.

`Channel.Create` is needed when setup creates the desired standard channel in
the selected Team. The signed-in user must also be allowed by Teams policy to
create channels there.

### Manual/Test

Provide:

- `tenant_id`
- `client_id`
- `refresh_token`, or test-only `access_token`
- optional default `team_id` + `channel_id`
- optional desired channel name for setup-time channel creation
- optional default `chat_id`

## Runtime Config

Required for normal sends:

- `tenant_id`
- `client_id`
- `MS_GRAPH_REFRESH_TOKEN` or test-only `MS_GRAPH_ACCESS_TOKEN`

Defaults:

- `graph_base_url`: `https://graph.microsoft.com/v1.0`
- `auth_base_url`: `https://login.microsoftonline.com`
- `token_scope`: `https://graph.microsoft.com/.default`

`public_base_url` is not required for egress-only. It is required for Graph change-notification subscriptions.

## Egress

The provider sends Graph `chatMessage` bodies:

```json
{
  "body": {
    "contentType": "text",
    "content": "hello"
  }
}
```

Supported destinations:

- Channel: `/teams/{team_id}/channels/{channel_id}/messages`
- Channel reply: `/teams/{team_id}/channels/{channel_id}/messages/{message_id}/replies`
- Chat: `/chats/{chat_id}/messages`

## Ingress

Ingress uses Microsoft Graph change notifications. The webhook endpoint must be publicly reachable over HTTPS. Subscription renewal is handled by the Teams ingress/subscription component.

## Limitations

- This is not a Bot Framework bot.
- No `@mention` command handling is provided by default.
- Messages are sent as the Graph-authorized identity according to Microsoft Graph capabilities and tenant policy.
- Adaptive Cards are fallback-rendered until a dedicated Teams card strategy is implemented.

## Checks

```bash
cargo test -p messaging-provider-teams-graph
cargo test -p messaging-ingress-teams
scripts/build_providers.sh teams
```

## Local Tester

Use the interactive Graph tester for OAuth, discovery, sends, subscriptions, and incoming webhook inspection:

```bash
scripts/test_teams.sh
```

Prerequisites:

- `cloudflared` for a public HTTPS webhook URL when testing subscriptions.
- A Microsoft Entra app registration with public client/device-code flow enabled.
- Delegated Graph permissions and consent for the operations you want to test.

No Azure Bot Service resource is required.

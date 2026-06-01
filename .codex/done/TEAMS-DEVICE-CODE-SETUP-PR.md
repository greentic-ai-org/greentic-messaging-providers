# PR: Align Teams Provider Pack With Device-Code Setup

## Title

Make Teams provider setup metadata consume generic Microsoft device-code setup

## Context

The Teams tester now proves the desired setup path:

1. Use Microsoft OAuth device code flow.
2. Use the `organizations` tenant endpoint by default.
3. Show `https://microsoft.com/devicelogin` and a `user_code`.
4. Poll Microsoft until access/refresh tokens are returned.
5. Discover `/me`, `/me/joinedTeams`, and `/teams/{team-id}/channels`.
6. Save Teams Graph config/secrets.
7. Send messages and receive Graph subscription notifications without Azure Bot Service.

This PR is the provider-pack side of that work. The generic setup engine support belongs in `greentic-setup`.

## Goal

Update Teams setup metadata, schemas, docs, and provider secret/config handling so once `gtc setup` supports generic device-code OAuth, the Teams provider works end to end without custom setup glue.

The default Teams path must remain:

```text
Connect Microsoft Teams
-> device-code login against organizations
-> admin/user consents
-> setup polls for tokens
-> setup discovers team/channel
-> setup stores Graph config/secrets
-> provider sends via Microsoft Graph
-> ingress uses Graph subscriptions
```

No Azure Bot Service, Bot Framework app ID/password, Bot Connector service URL, or Bot Connector conversation ID should be required in the default path.

## Provider Metadata Contract

Add or update Teams pack/setup metadata to declare a generic device-code OAuth action. Prefer an explicit extension that `greentic-setup` can consume, for example:

```json
{
  "kind": "oauth_device_code",
  "provider": "microsoft",
  "tenant_alias": "organizations",
  "device_code_url": "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode",
  "token_url": "https://login.microsoftonline.com/organizations/oauth2/v2.0/token",
  "verification_uri": "https://microsoft.com/devicelogin",
  "client_id_config_key": "client_id",
  "client_id_secret_key": "MS_GRAPH_CLIENT_ID",
  "scopes": [
    "offline_access",
    "openid",
    "profile",
    "User.Read",
    "Team.ReadBasic.All",
    "Channel.ReadBasic.All",
    "ChannelMessage.Send",
    "ChannelMessage.Read.All",
    "Chat.Read"
  ],
  "secrets_out": {
    "client_id": "MS_GRAPH_CLIENT_ID",
    "refresh_token": "MS_GRAPH_REFRESH_TOKEN",
    "access_token": "MS_GRAPH_ACCESS_TOKEN"
  },
  "config_out": {
    "tenant_id": "tenant_id",
    "client_id": "client_id",
    "user_id": "user_id",
    "team_id": "team_id",
    "channel_id": "channel_id",
    "chat_id": "chat_id"
  },
  "post_login_discovery": [
    {
      "id": "me",
      "method": "GET",
      "url": "https://graph.microsoft.com/v1.0/me"
    },
    {
      "id": "joined_teams",
      "method": "GET",
      "url": "https://graph.microsoft.com/v1.0/me/joinedTeams"
    },
    {
      "id": "channels",
      "method": "GET",
      "url_template": "https://graph.microsoft.com/v1.0/teams/{team_id}/channels",
      "requires": ["team_id"]
    }
  ]
}
```

Exact JSON/YAML shape may be adapted to the existing pack extension conventions, but the data must be generic enough for `greentic-setup` to execute without Teams-specific Rust code.

## Files Likely In Scope

- `packs/messaging-teams/pack.yaml`
- `packs/messaging-teams/pack.manifest.json`
- `packs/messaging-teams/assets/setup.yaml`
- `packs/messaging-teams/secret-requirements.json`
- `packs/messaging-teams/.secret_requirements.json`
- `schemas/messaging/teams/config.schema.json`
- `schemas/messaging/teams/public.config.schema.json`
- pack/component copied schema files under `packs/messaging-teams/**/schemas`
- `components/messaging-provider-teams/src/config.rs`
- `components/messaging-provider-teams/src/auth.rs`
- `components/messaging-provider-teams/src/describe.rs`
- docs:
  - `docs/providers/teams.md`
  - `docs/guides/providers/guide-teams-setup.md`
  - `packs/messaging-teams/README.md`
- fixtures:
  - `packs/messaging-teams/fixtures/setup.input.json`
  - `packs/messaging-teams/fixtures/setup.expected.plan.json`
  - `packs/messaging-teams/fixtures/requirements.expected.json`

## Runtime Checks

Verify provider runtime accepts the secrets/config produced by setup:

- `MS_GRAPH_CLIENT_ID`
- `MS_GRAPH_REFRESH_TOKEN`
- optional `MS_GRAPH_ACCESS_TOKEN` for test/dev override
- no default `MS_GRAPH_CLIENT_SECRET`
- `tenant_id`
- `client_id`
- optional `user_id`
- `team_id` + `channel_id`, or `chat_id`
- `public_base_url` only for subscriptions/webhooks

If `components/messaging-provider-teams/src/auth.rs` only reads `client_id` from config, update it to also resolve `MS_GRAPH_CLIENT_ID` from secrets/env. The tester stores this key and `gtc setup` should do the same.

## Setup QA / FormSpec Behavior

Teams setup should not ask for Bot Framework fields:

- remove `MS_BOT_APP_ID`
- remove `MS_BOT_APP_PASSWORD`
- remove `default_service_url`
- remove Bot Connector conversation/service URL fields

Teams setup should ask only:

- Microsoft `client_id` if not provided by config/env/hosted setup
- optional manual fallback fields for `tenant_id`, `refresh_token`, `access_token`
- team/channel/chat selection after device-code login and discovery
- `public_base_url` only when Graph subscriptions are enabled

The default tenant alias for OAuth setup must be `organizations`. The discovered tenant ID from the token/user/team should still be saved as `tenant_id` for provider runtime.

## Scopes

Use these default delegated scopes:

```text
offline_access openid profile User.Read Team.ReadBasic.All Channel.ReadBasic.All ChannelMessage.Send ChannelMessage.Read.All Chat.Read
```

Rationale:

- `ChannelMessage.Send` is needed for channel egress.
- `ChannelMessage.Read.All` is needed for channel message Graph subscriptions and notification fetch/enrichment.
- `Chat.Read` is needed for chat subscriptions/fetch paths.
- Some tenants require admin consent for read scopes; docs/setup errors should say this clearly.

## Docs

Update docs to make the new setup path explicit:

- no redirect URL is required for the default device-code login
- no app registration is created by the provider
- no Azure Bot Service is required
- no Bot Framework is required
- app registration must be multi-tenant for cross-organization users:
  `signInAudience = AzureADMultipleOrgs`
- public client/device-code flow must be enabled
- tenant may require admin consent
- Graph subscriptions need a public HTTPS `public_base_url` and message read consent

## Tests / Validation

Run or update:

- `cargo test -p messaging-provider-teams`
- `cargo test -p messaging-ingress-teams`
- `python3 tools/validate_pack_fixtures.py packs/messaging-teams`
- `scripts/build_providers.sh teams`

Update fixtures and generated pack artifacts only through the repo's normal generation/build flow.

## Acceptance Criteria

- Teams pack declares generic Microsoft device-code OAuth setup metadata.
- Teams setup metadata defaults to `organizations`.
- Teams default setup no longer uses redirect/callback OAuth.
- Teams default setup does not ask for client secret.
- Teams default setup does not ask for Bot Framework/Bot Service fields.
- Provider runtime accepts `MS_GRAPH_CLIENT_ID` + `MS_GRAPH_REFRESH_TOKEN` produced by setup.
- Discovery output can persist `tenant_id`, `user_id`, `team_id`, `channel_id`, and/or `chat_id`.
- Docs describe admin consent and multi-tenant app requirements.
- Existing Teams build/tests/fixtures pass.

## Out Of Scope

- Implementing generic device-code UI/polling in `greentic-setup`; that belongs in the paired `greentic-setup` PR.
- Reintroducing app-catalog install as a setup requirement.
- Creating Microsoft app registrations automatically.

# PR 2: Simplify Teams Setup To Microsoft Graph OAuth

## Review Status

Reviewed against the current codebase on 2026-05-27 and adapted.

This PR is necessary after PR 1. The current repo has conflicting Teams setup/config sources:

- `components/messaging-provider-teams/src/describe.rs` asks for Bot Framework fields and has `DEFAULT_KEYS = ["ms_bot_app_id", "public_base_url"]`.
- `packs/messaging-teams/assets/setup.yaml` is titled "Microsoft Teams Bot Framework provider setup" and asks for `ms_bot_app_id`, `ms_bot_app_password`, and `public_base_url`.
- `packs/messaging-teams/secret-requirements.json` contains both Graph and Bot secrets.
- `packs/messaging-teams/pack.yaml` and `pack.manifest.json` describe `messaging.teams.bot`.
- Root `schemas/messaging/teams/config.schema.json` is Graph-shaped, but pack-local schemas are split: `packs/messaging-teams/schemas/.../config.schema.json` is Bot-shaped, while public/assets schemas are Graph-shaped.
- Docs are mixed: some already discuss Graph egress, but provider docs and pack README still describe Azure Bot Service / Bot Framework as central.

## Title

Simplify Teams setup to Microsoft Graph OAuth

## Goal

Make Teams setup Graph OAuth-first:

1. Login with Microsoft.
2. Approve Graph permissions.
3. Choose Team/Channel or Chat.
4. Save derived config/secrets.

Do not ask for `MS_BOT_APP_ID`, `MS_BOT_APP_PASSWORD`, `default_service_url`, Azure Bot resource, or Bot Connector endpoint in the default setup.

## Adapted Scope

Update setup, schemas, pack metadata, fixtures, and docs after PR 1 establishes Graph egress behavior.

Likely files:

- `components/messaging-provider-teams/src/describe.rs`
- `components/messaging-provider-teams/src/config.rs`
- `components/messaging-provider-teams/component.manifest.json`
- `packs/messaging-teams/pack.yaml`
- `packs/messaging-teams/pack.manifest.json`
- `packs/messaging-teams/assets/setup.yaml`
- `packs/messaging-teams/secret-requirements.json`
- `schemas/messaging/teams/*.schema.json`
- `packs/messaging-teams/schemas/messaging/teams/*.schema.json`
- `packs/messaging-teams/assets/schemas/messaging/teams/*.schema.json`
- `components/messaging-provider-teams/schemas/messaging/teams/config.schema.json`
- Teams fixtures under `packs/messaging-teams/fixtures/`
- `docs/providers/teams.md`
- `docs/providers/README.md`
- `docs/guides/providers/guide-teams-setup.md`
- `docs/guides/providers/03-provider-status.md`
- `docs/guides/providers/06-messaging-providers-deep-dive.md` if it references Bot Framework fields.
- `packs/messaging-teams/README.md`

## Setup QA Design

Default questions should become:

- `enabled`: optional, default true
- `tenant_id`: optional/manual fallback
- `client_id`: optional/manual fallback
- `refresh_token`: optional/manual fallback secret
- `access_token`: optional/test-only secret
- `public_base_url`: optional, required only for Graph subscriptions/ingress
- `team_id`: optional default
- `channel_id`: optional default
- `chat_id`: optional default

If the QA model cannot represent an OAuth login action directly, keep manual fallback fields and make docs/tester responsible for populating them from OAuth.

Use Graph-first defaults:

```text
DEFAULT_KEYS = ["tenant_id", "client_id"]
```

Runtime validation should still require one token source for actual sends:

- config/secrets refresh token, or
- test/dev access-token override.

## Pack Metadata

Change description to:

```text
Microsoft Teams messaging provider - Microsoft Graph egress and Graph change notification ingress
```

Change provider type:

```text
messaging.teams.graph
```

Keep operations that are actually supported by the correct component boundary:

- Provider component:
  - `send`
  - `reply`
  - `ingest_http` only if still retained as legacy/provider-local normalization
  - `render_plan`
  - `encode`
  - `send_payload`
  - `qa-spec`
  - `apply-answers`
  - `i18n-keys`
- Ingress/subscription component:
  - `sync-subscriptions` through `messaging.subscriptions.v1`

Do not leave `subscription_ensure`, `subscription_renew`, or `subscription_delete` advertised as provider runtime ops unless PR 3 adds matching exported operations.

## OAuth Extension

Update `messaging.oauth.v1`:

- `provider_id: teams`
- `authorize_url: https://login.microsoftonline.com/common/oauth2/v2.0/authorize`
- `token_url: https://login.microsoftonline.com/common/oauth2/v2.0/token`
- `redirect_path: /oauth/callback/teams`
- scopes should be explicit unless the platform requires `.default`.

Suggested delegated scopes to verify against current Microsoft docs during implementation:

- `offline_access`
- `openid`
- `profile`
- `User.Read`
- `Team.ReadBasic.All`
- `Channel.ReadBasic.All`
- `ChannelMessage.Send`
- chat-send/read scopes supported by the implemented chat path.

If `.default` is retained, document that app permissions must be preconfigured in Entra.

Secret keys:

- `MS_GRAPH_TENANT_ID`
- `MS_GRAPH_CLIENT_ID`
- `MS_GRAPH_REFRESH_TOKEN`
- `MS_GRAPH_CLIENT_SECRET` optional
- `MS_GRAPH_ACCESS_TOKEN` optional/test-only

Remove Bot secrets from default requirements:

- `MS_BOT_APP_ID`
- `MS_BOT_APP_PASSWORD`

## Docs Rewrite

Reframe Teams as:

- No Azure Bot Service required.
- Uses Microsoft Graph delegated OAuth for sending messages.
- Uses Graph change notifications for ingress once PR 3 is implemented.

Document setup modes:

1. Hosted/SaaS: click Login with Microsoft; client ID hidden or managed by service.
2. Self-hosted: create/register Entra app once, then login and consent.
3. Manual/test: paste `tenant_id`, `client_id`, `refresh_token`, or `access_token`.

Document limitations:

- Not a native Bot Framework bot.
- No Bot Framework command handling by default.
- Messages are sent as the Graph-authorized identity according to Microsoft Graph capabilities.
- Adaptive Cards may be fallback-rendered until a dedicated Teams card strategy exists.
- Public HTTPS callback is required for Graph subscriptions, but not for egress-only.

Remove default Azure Bot Service setup steps and "configure messaging endpoint in Azure Portal" from the main path.

## Fixtures And Generated Artifacts

Update fixtures that currently expect Bot fields:

- `packs/messaging-teams/fixtures/setup.input.json`
- `packs/messaging-teams/fixtures/setup.expected.plan.json`
- `packs/messaging-teams/fixtures/requirements.expected.json`
- `packs/messaging-teams/fixtures/egress.request.json`
- `packs/messaging-teams/fixtures/egress.expected.summary.json`
- `packs/messaging-teams/fixtures/ingress.expected.message.json` if provider type changes.

Regenerate pack manifests/locks only if this repository's normal pack sync process requires it for CI. Avoid manual generated drift.

## Acceptance Criteria

- `gtc setup` / QA no longer asks for Bot App ID or Bot App Password by default.
- Pack metadata no longer advertises Teams as Bot Framework.
- Schema files agree on Graph config fields.
- Docs clearly state Azure Bot Service is not required.
- Fixture/schema/pack validation tests pass.

## Out Of Scope

- Rewriting egress code. That belongs in PR 1.
- Implementing Graph notification normalization. That belongs in PR 3.
- Adding an interactive tester. That belongs in PR 4.

# PR - Teams Generic Setup Contract

## Title

Declare Teams setup through generic setup web-component and backend-contract metadata

## Context

The Teams setup flow is currently implemented as a Bot Framework-compatible setup wizard. It is not using a separate `setup-machine.v1.json` asset, and the generated pack does not advertise a `greentic.setup.machine.v1` extension.

The current source of truth is:

- `messaging-teams/assets/setup/backend-contract.json`
- `messaging-teams/assets/setup/greentic-teams-setup.js`
- `messaging-teams/build-answer.json`
- `messaging-teams/build_pack.sh`
- `components/messaging-ingress-teams/src/setup.rs`

The provider pack already declares generic setup metadata through:

- `greentic.setup.web-component.v1`
- `greentic.setup.backend-contract.v1`
- `greentic.static-routes.v1`
- `greentic.http-routes.v1`

This PR should harden that existing contract rather than introduce a new setup-machine file unless the matching `greentic-setup` schema and executor exist.

## Goal

Make `messaging-teams` a reference provider for generic, resumable setup using the existing setup web-component and backend-contract model so a setup host can:

- discover and mount the Teams setup UI from pack metadata
- call generic setup routes without Teams-specific setup-host code
- run the declared setup actions in order
- resume setup after process/browser/server restarts from persisted setup state
- recover from known Teams setup failures such as device-code expiry, stale runtime tunnel/public URL, transient provider HTTP failures, and incomplete manual Teams install/chat steps
- keep OAuth/device-code state server-owned and prevent browser-submitted secrets from overwriting setup state

## Current Provider Contract

The generated Teams pack should expose:

```yaml
extensions:
  greentic.setup.web-component.v1:
    kind: greentic.setup.web-component.v1
    version: "1"
    inline:
      schema_id: greentic.setup.web-component.v1
      provider_id: messaging-teams
      tag_name: greentic-teams-setup-v4
      module_asset: assets/setup/greentic-teams-setup.js
      module_url: /v1/web/messaging-teams/setup/{tenant}/greentic-teams-setup.js?v=0.5.17-setup4
      asset_base_path: /v1/web/messaging-teams/setup/{tenant}
      completion:
        event: greentic-provider-setup-complete
        state_event: greentic-provider-setup-state
        state_path: setup_status.ok
        equals: true
  greentic.setup.backend-contract.v1:
    kind: greentic.setup.backend-contract.v1
    version: "1"
    inline:
      schema_id: greentic.setup.backend-contract.v1
      provider_id: messaging-teams
      asset: assets/setup/backend-contract.json
```

`messaging-teams/build-answer.json` is the source for the web-component descriptor. `messaging-teams/build_pack.sh` copies setup assets into `target/generated/messaging-teams.pack`, injects the setup extensions, adds static routes for `assets/setup`, and declares provider HTTP setup routes.

Do not add `messaging-teams/assets/setup/setup-machine.v1.json` or `greentic.setup.machine.v1` unless `greentic-setup` has landed that schema and this repo can validate it. The current codebase has no such asset or extension.

## Current Setup Sequence

The runtime setup state machine in `components/messaging-ingress-teams/src/setup.rs` should match `messaging-teams/assets/setup/backend-contract.json` and the Playwright tests. The implemented setup flow has seven visible steps:

1. `graph_admin_consent`
   - Microsoft device-code OAuth against `organizations` by default.
   - Uses the Microsoft Graph Command Line Tools public client by default:
     `14d82eec-204b-4c2f-b7e8-296a70dab67e`.
   - Requests setup-time Graph permissions for app registration, Teams app catalog publish/install, and user identity.

2. `bot_app_identity`
   - Creates or reuses the Microsoft Entra app used by Bot Framework.
   - Stores `bot_app_id` and generated/supplied `bot_app_password` in setup config/state.

3. `microsoft_bot_channel_registration_consent`
   - Uses Microsoft device-code OAuth for Azure management scope:
     `https://management.azure.com/user_impersonation`.
   - Stores `azure_management_access_token`.
   - Authorizes Bot Framework/Teams channel registration through Azure management APIs.

4. `bot_framework_endpoint_registration`
   - Registers or updates the Bot Framework-compatible Teams endpoint.
   - Uses the provider HTTP route:
     `/v1/setup/messaging-teams/{tenant}/{team}/bot-framework-registration`.
   - Requires `public_base_url` because Teams must reach the active runtime ingress URL.

5. `teams_app_publish`
   - Builds the Teams app package from `messaging-teams/assets/teams-app/manifest.template.json`.
   - Publishes or reuses the app in the Teams app catalog through a provider HTTP action:
     `/v1/setup/messaging-teams/{tenant}/{team}/teams-app-publish`.

6. `teams_app_user_install`
   - Installs or reuses the published app for the setup user.
   - Exposes Add to Teams and Open bot chat links for manual fallback.

7. `first_bot_framework_post`
   - Waits for a real inbound Bot Framework activity received by `greentic-start`.
   - Setup is not complete when the app is merely installed; completion requires the first Teams message proof.

## Executor Kinds

The backend contract currently depends on these generic executor kinds:

- `oauth_device_code`
- `microsoft_graph_application`
- `provider_http`
- `runtime_observation`

`greentic-setup` should implement those executor kinds generically. It must not contain Teams-specific branches for Bot Framework registration, Teams app publish/install, or first-message observation. Provider-owned HTTP actions are routed through the pack's `greentic.http-routes.v1` declarations and dispatched to `messaging-ingress-teams`.

## OAuth And Azure Details

The current Graph OAuth action declares:

```json
{
  "provider": "microsoft_identity",
  "authority_url_template": "https://login.microsoftonline.com/{authority_tenant}",
  "authority_tenant_config_key": "azure_auth_tenant",
  "authority_tenant_default": "organizations",
  "client_id_config_key": "graph_setup_client_id",
  "client_id_default": "14d82eec-204b-4c2f-b7e8-296a70dab67e",
  "client_id_default_name": "Microsoft Graph Command Line Tools",
  "scopes": [
    "https://graph.microsoft.com/Application.ReadWrite.All",
    "https://graph.microsoft.com/AppCatalog.ReadWrite.All",
    "https://graph.microsoft.com/TeamsAppInstallation.ReadWriteForUser",
    "https://graph.microsoft.com/User.Read"
  ],
  "oauth_kind": "graph",
  "token_store_key": "graph_access_token"
}
```

The backend contract includes a Microsoft Bot Channel registration consent action for Azure management:

```json
{
  "oauth_kind": "management",
  "token_store_key": "azure_management_access_token",
  "scopes": [
    "https://management.azure.com/user_impersonation"
  ]
}
```

Keep browser-submitted OAuth/device-code values server-owned. `backend-contract.json` currently lists these server-owned config keys:

- `oauth_kind`
- `oauth_device_code`
- `oauth_user_code`
- `azure_management_device_code`
- `azure_management_user_code`
- `graph_access_token`
- `azure_management_access_token`
- `bot_access_token`

## Runtime Configuration

Do not assume the default interactive setup path is Graph-subscription based. The current setup path requires Bot Framework configuration:

- `public_base_url`
- `bot_app_id`
- `bot_app_password`
- `bot_display_name`
- optional Azure registration fields such as `azure_subscription_id`, `azure_resource_group`, `azure_location`, and `azure_bot_name`

The component still has Graph subscription lifecycle code for provider operations, and the Teams provider schemas still include Graph config such as `tenant_id`, `client_id`, `team_id`, `channel_id`, `chat_id`, `graph_base_url`, `auth_base_url`, `token_scope`, `refresh_token`, and `access_token`. Treat that as separate from the current setup wizard unless a follow-up explicitly wires Graph subscription setup into the generic backend contract.

The previous assumption that Teams setup "must not require Bot Framework app password, Bot Connector service URL, or Bot Connector conversation ID in the default path" is wrong for this codebase. The active setup flow is explicitly Bot Framework-compatible and uses `bot_app_password` plus the runtime ingress endpoint. It does not require a pre-existing Bot Connector conversation ID; it waits for the first real inbound activity instead.

## Recovery Rules

Declare and test recovery behavior through `backend-contract.json`, setup state, and web-component behavior:

- `device_code_expired` -> refresh code and continue device-code OAuth.
- `authorization_pending` -> keep the Microsoft sign-in action visible and poll/continue.
- `oauth_denied` or consent changed -> restart device-code login with admin guidance.
- `provider_http_transient_failure` -> show backend diagnostics and retry the same step without progress regression.
- `public_base_url_missing` -> block Bot Framework registration until a valid public runtime URL is supplied.
- `public_endpoint_changed` or stale tunnel -> rerun Bot Framework endpoint registration before offering Teams app/chat verification.
- `teams_app_publish_conflict` -> find/reuse the existing Teams app by external ID where possible.
- `manual_install_incomplete` -> keep setup paused after Teams app publish/install and keep Add to Teams/Open bot chat links visible.
- `first_activity_missing` -> keep setup at `first_bot_framework_post` until `greentic-start` records a real Bot Framework activity.

All recoveries should be bounded by retry/timeout policy in the setup host or component.

## Build And Validation

Provider-side validation should target the current artifacts:

- JSON syntax validation for `messaging-teams/assets/setup/backend-contract.json`.
- JSON syntax validation for `messaging-teams/assets/setup/conformance.json`.
- Build validation through `messaging-teams/build_pack.sh`.
- Consistency validation that `backend-contract.json` required order matches the runtime/UI setup steps.
- Pack metadata checks that the generated pack includes:
  - `assets/setup/greentic-teams-setup.js`
  - `assets/setup/backend-contract.json`
  - `assets/setup/conformance.json`
  - `assets/setup.routes.json`
  - `greentic.setup.web-component.v1`
  - `greentic.setup.backend-contract.v1`
  - `greentic.http-routes.v1`
- Playwright coverage in `tests/messaging-teams-setup/specs/wizard.spec.ts`.
- Full local validation through `./ci/local_check.sh` and `greentic-dev coverage`.
- ES-module syntax validation for the setup asset with
  `node --input-type=module --check < messaging-teams/assets/setup/greentic-teams-setup.js`.

Once `greentic-setup doctor provider <pack>` supports this backend-contract model, use it as the authoritative contract test. Until then, do not make CI depend on `greentic.setup.machine.v1`.

## Migration Work

This PR should not remove the existing setup web component or backend contract. It should instead finish aligning implementation, generated metadata, docs, and tests around the current generic setup contract.

Migration and cleanup work:

- Keep `microsoft_bot_channel_registration_consent` aligned across `backend-contract.json`, `setup.rs`, `greentic-teams-setup.js`, and tests.
- Keep `messaging-teams/assets/setup/backend-contract.json` aligned with the runtime setup steps in `components/messaging-ingress-teams/src/setup.rs`.
- Ensure `messaging-teams/build-answer.json` and `messaging-teams/build_pack.sh` advertise `greentic-teams-setup-v4`, not the older tag name.
- Ensure generated pack metadata points to `assets/setup/backend-contract.json`.
- Ensure provider HTTP route metadata covers Bot Framework registration, Teams app publish, and Teams app install.
- Keep server-owned OAuth/device-code state out of browser-submitted config.
- Update docs that still describe the setup flow as Graph-subscription first or omit the Bot Framework setup path.
- Keep Graph subscription fixtures/tests separate from the Bot Framework setup wizard unless the backend contract declares them.

## Fixtures And Tests

Current and useful fixture/test scenarios include:

- fresh setup completes against a fake backend
- device-code login starts, refreshes, and completes
- generic pending state renders Continue setup and posts `next`
- missing `public_base_url` blocks Bot Framework registration
- stale runtime tunnel/public URL blocks final bot-message verification
- provider HTTP failures display diagnostics and retry advances the step
- Teams app publish transient failures retry
- installed Teams app waits for a real bot message
- browser-submitted OAuth/device-code fields are ignored

Avoid real secrets. Use redacted or synthetic OAuth codes, tokens, bot app IDs, and runtime URLs.

## Docs

Update Teams provider/setup docs to explain:

- setup is currently driven by `greentic.setup.web-component.v1` and `greentic.setup.backend-contract.v1`
- the setup UI is `greentic-teams-setup-v4`
- setup has seven visible Bot Framework-compatible steps
- the Microsoft Graph and Azure management OAuth permissions requested by setup
- why `public_base_url` must be reachable from Teams
- why setup is not complete until the first Bot Framework activity is received
- how manual Teams install/open-chat fallback works
- which Graph subscription functionality exists outside the default setup wizard
- when `greentic-setup doctor provider <pack>` is expected to become authoritative

## Files Likely Touched

- `.codex/PR-TEAMS-SETUP-MACHINE.md`
- `messaging-teams/build-answer.json`
- `messaging-teams/build_pack.sh`
- `messaging-teams/assets/setup/README.md`
- `messaging-teams/assets/setup/conformance.json`
- `messaging-teams/assets/setup/backend-contract.json`
- `messaging-teams/assets/setup/greentic-teams-setup.js`
- `components/messaging-ingress-teams/src/setup.rs`
- `components/messaging-ingress-teams/src/teams_pkg.rs`
- `crates/provider-common/tests/pack_metadata.rs`
- `tests/messaging-teams-setup/fixtures/server.mjs`
- `tests/messaging-teams-setup/specs/wizard.spec.ts`
- `docs/providers/teams.md`
- `docs/guides/providers/guide-teams-setup.md`

## Acceptance Criteria

- Generated `messaging-teams.gtpack` exposes `greentic.setup.web-component.v1` and `greentic.setup.backend-contract.v1`.
- Generated metadata references `greentic-teams-setup-v4` and `assets/setup/backend-contract.json`.
- `backend-contract.json` declares the current setup action order and generic executor kinds.
- `backend-contract.json` and `setup.rs` agree on setup step order, including Azure management consent as a separate user-visible step.
- `greentic.http-routes.v1` includes provider HTTP routes for Bot Framework registration, Teams app publish, and Teams app install.
- Setup state can resume without trusting browser-submitted OAuth/device-code fields.
- The UI handles device-code refresh, provider HTTP retry, missing public URL, stale runtime tunnel, manual install/chat fallback, and first-message completion.
- Provider tests verify setup metadata and assets are included in generated pack output.
- Docs no longer describe the active setup wizard as a `setup-machine.v1.json` or Graph-subscription-first flow.

## Out Of Scope

- Implementing a new generic `setup-machine.v1` executor in this provider repo.
- Adding a `greentic.setup.machine.v1` extension before `greentic-setup` defines and validates it.
- Removing the current setup web component.
- Replacing the Bot Framework-compatible setup path with Graph subscriptions.
- Performing live Microsoft Graph, Azure, or Teams end-to-end validation in unit tests.

## Dependencies

This PR depends on `greentic-setup` supporting the generic setup contract already declared by the Teams pack:

- `greentic.setup.web-component.v1`
- `greentic.setup.backend-contract.v1`
- `greentic.setup.actions.v1`
- `oauth_device_code`
- `microsoft_graph_application`
- `provider_http`
- `runtime_observation`

If a later `greentic-setup` PR introduces `greentic.setup.machine.v1`, handle that as a follow-up migration with an explicit schema, generated-pack validation, and compatibility plan for the existing backend-contract/web-component metadata.

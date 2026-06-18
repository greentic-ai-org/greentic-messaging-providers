# Provider Setup Web Components

Provider packs can expose an embedded setup experience through the generic
`greentic.setup.web-component.v1` extension. Setup hosts should use this
descriptor instead of hardcoding provider-specific setup screens.

## Descriptor

The extension inline payload, or `assets/setup.routes.json` for unpacked assets,
declares:

```json
{
  "schema_id": "greentic.setup.web-component.v1",
  "provider_id": "messaging-teams",
  "tag_name": "greentic-teams-setup",
  "module_asset": "assets/setup/greentic-teams-setup.js",
  "module_url": "/v1/web/messaging-teams/setup/{tenant}/greentic-teams-setup.js",
  "asset_base_path": "/v1/web/messaging-teams/setup/{tenant}",
  "attributes": {
    "provider-id": "messaging-teams",
    "api-base": "",
    "locale": "{locale}",
    "state-path": "/v1/messaging/setup/messaging-teams/{tenant}",
    "next-path": "/v1/messaging/setup/messaging-teams/{tenant}/next"
  },
  "events": {
    "state": "greentic-provider-setup-state",
    "complete": "greentic-provider-setup-complete"
  },
  "completion": {
    "event": "greentic-provider-setup-complete",
    "state_event": "greentic-provider-setup-state",
    "state_path": "setup_status.ok",
    "equals": true
  }
}
```

Hosts replace placeholders such as `{tenant}` and `{locale}`, import the module
from `module_url` or by resolving `module_asset` through the static route, create
the custom element named by `tag_name`, and apply the descriptor attributes. The
host does not need to understand provider-specific steps, buttons, OAuth device
login, role guidance, or installation links.

## Setup Backend

The web component is not the setup backend. Setup API routes such as
`/v1/messaging/setup/{provider}/{tenant}/next` are owned by `greentic-setup`
(`gtc setup`). They should perform admin setup, persist setup outputs into the
bundle/tenant configuration, and prepare the provider for runtime use.

Packs can declare `greentic.setup.backend-contract.v1` to describe what
`greentic-setup` must implement for a provider. Hosts should treat this as
provider metadata, not UI logic. Contracts that declare
`actions_schema_id: greentic.setup.actions.v1` also map each required step to a
generic executor kind. For example, `messaging-teams` uses generic executors for
Microsoft device-code OAuth, Microsoft Graph application management, Bot
Framework registration, Teams app catalog publishing, Teams user installation,
and runtime observation. The pack supplies provider-specific assets, config
keys, scopes, and state paths; `greentic-setup` supplies reusable executor
implementations.

For example, `messaging-teams` declares that setup must keep OAuth/device-code
state server side, ignore browser-submitted token fields, register the Bot
Framework-compatible endpoint before exposing Teams install/chat actions, and
wait for `greentic-start` to report the first Bot Framework POST before
reporting setup complete.

`greentic-start` (`gtc start`) should focus on the configured provider runtime:
message ingress, egress, and routing. It should not own the admin setup wizard.

## Completion

The host should mark setup complete when either condition is true:

- the configured completion event fires, usually `greentic-provider-setup-complete`
- the latest configured state event has `setup_status.ok === true`

All generic events include `detail.providerId`, so one admin page can host
multiple provider setup components without custom event names.

## Fallback

If a pack does not declare `greentic.setup.web-component.v1`, setup hosts should
fall back to existing schema-driven setup forms and action buttons.

## Capabilities

Packs that expose setup web components should still declare
`greentic.ext.capabilities.v1` so bundle/setup discovery can identify them as
messaging provider packs. Ingress-only setup packs do not need to declare
`greentic.provider-extension.v1`; that extension is for schema-core provider
components. Older `greentic-pack doctor` versions may therefore print
`Providers: none` even when `greentic.ext.capabilities.v1`,
`greentic.setup.web-component.v1`, and `messaging.provider_ingress.v1` are
present and valid.

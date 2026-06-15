# Greentic Teams Setup Web Component

`<greentic-teams-setup>` embeds the Teams admin setup flow in a portal or local
tester page. It follows the generic Greentic provider setup web-component
contract, so a setup host can discover and mount it from pack metadata without
hardcoding Teams behavior. The default UI is a compact guided wizard: progress,
next action, one primary action, Microsoft device-login instructions, and
admin-blocked guidance.

Only one action is shown at a time. The component chooses the current action in
this order: Microsoft sign-in, refresh after manual admin action, Add to Teams
when the app is published but not installed, Open bot chat when installation is
done but no message has arrived, continue setup for backend steps, then package
download only as a fallback.

Clicking the action starts a managed step. If Microsoft device login is needed,
the component first shows the user code and a copy button; the admin then clicks
Open Microsoft device login. A secondary Refresh code button is available only
for device login, so a bad or expired code can be replaced immediately without
hiding the current code panel. The component shows a working state for managed
steps, polls for progress, and either advances to the next visible step or shows
a timeout with a Retry action. For device login, the timeout action also
refreshes the code. Use `action-timeout` to control the timeout window.

## Quick Embed

Hosts should prefer the pack-provided `greentic.setup.web-component.v1`
extension, or `assets/setup.routes.json` when inspecting unpacked assets. That
descriptor provides the custom element tag name, JavaScript module asset,
attributes, generic event names, and completion condition.

```html
<script type="module" src="/v1/web/messaging-teams/setup/default/greentic-teams-setup.js"></script>

<greentic-teams-setup
  provider-id="messaging-teams"
  api-base="https://admin.example.com"
  locale="en"
  state-path="/v1/messaging/setup/messaging-teams/default"
  next-path="/v1/messaging/setup/messaging-teams/default/next"
  config-path="/v1/messaging/setup/messaging-teams/default/config"
  oauth-start-path="/v1/messaging/setup/messaging-teams/default/oauth/{kind}/start"
  oauth-complete-path="/v1/messaging/setup/messaging-teams/default/oauth/{kind}/complete"
  package-path="/v1/messaging/setup/messaging-teams/default/teams-app/package.zip">
</greentic-teams-setup>
```

By default the component uses the local tester API. Provider-owned setup
endpoints should be supplied through the endpoint path attributes shown above.
The component renders device-login codes, manual admin role guidance, Teams
install links, and configuration fields.

Provider-owned setup endpoints can be used without changing the component code:

```html
<greentic-teams-setup
  provider-id="messaging-teams"
  api-base="https://admin.example.com"
  state-path="/v1/messaging/setup/messaging-teams/default"
  next-path="/v1/messaging/setup/messaging-teams/default/next"
  config-path="/v1/messaging/setup/messaging-teams/default/config"
  oauth-start-path="/v1/messaging/setup/messaging-teams/default/oauth/{kind}/start"
  oauth-complete-path="/v1/messaging/setup/messaging-teams/default/oauth/{kind}/complete"
  package-path="/v1/messaging/setup/messaging-teams/default/teams-app/package.zip">
</greentic-teams-setup>
```

Configuration fields are hidden by default. Add `advanced` only in engineering
or support surfaces:

```html
<greentic-teams-setup api-base="http://127.0.0.1:8793" advanced></greentic-teams-setup>
```

## Theming

Set CSS custom properties on the element or a parent container:

```css
greentic-teams-setup {
  --gts-accent: #2563eb;
  --gts-accent-hover: #1d4ed8;
  --gts-accent-soft: #dbeafe;
  --gts-radius: 6px;
  --gts-panel: #ffffff;
  --gts-page: #f6f8fb;
}
```

## i18n

Built-in locales: `en`, `nl`.

Portal code can override or add translations:

```html
<greentic-teams-setup id="teams-setup" locale="fr"></greentic-teams-setup>

<script type="module">
  import { GreenticTeamsSetup } from "./greentic-teams-setup.js";

  GreenticTeamsSetup.translations.fr = {
    title: "Configuration Teams",
    runNext: "Executer l'etape suivante"
  };

  document.getElementById("teams-setup").translations = {
    subtitle: "Texte specifique au portail."
  };
</script>
```

Overrides are partial; missing strings fall back to English.

## Attributes

| Attribute | Purpose |
| --- | --- |
| `api-base` | Base URL for setup API calls. Empty means same origin. |
| `locale` | Locale key, such as `en`, `nl`, or a portal-provided locale. |
| `auto-poll` | `true` or `false`. Defaults to `true`. |
| `poll-interval` | Poll interval in milliseconds. Defaults to `3000`. |
| `advanced` | Shows raw configuration fields and last setup result. Hidden by default. |
| `action-timeout` | Milliseconds to wait for a clicked non-OAuth action to advance. Defaults to `120000`. Device-login actions wait until the Microsoft device code expiry window, typically about 15 minutes. |
| `state-path` | State endpoint. Defaults to `/api/state`. |
| `next-path` | One-step setup endpoint. Defaults to `/api/setup/next`. |
| `config-path` | Configuration save endpoint. Defaults to `/api/config`. |
| `oauth-start-path` | Device-code start endpoint with `{kind}` placeholder. |
| `oauth-complete-path` | Device-code completion endpoint with `{kind}` placeholder. |
| `package-path` | Teams app package download endpoint. Defaults to `/teams-app/package.zip`. |

## Events

All events bubble and cross the Shadow DOM boundary.

The generic events below are the stable host integration contract for any
provider-supplied setup web component:

| Event | Detail |
| --- | --- |
| `greentic-provider-setup-state` | `{ providerId, state }` after the configured state endpoint loads. |
| `greentic-provider-setup-result` | `{ providerId, result }` after setup/config actions. |
| `greentic-provider-setup-action-start` | `{ providerId, action, before }` when the visible action starts. |
| `greentic-provider-setup-action-complete` | `{ providerId, action, state }` when the action advances setup. |
| `greentic-provider-setup-action-timeout` | `{ providerId, action, error }` when no next step appears before timeout. |
| `greentic-provider-setup-device-login` | `{ providerId, login }` when a device-code login is needed. |
| `greentic-provider-setup-error` | `{ providerId, error }` when an API call fails. |
| `greentic-provider-setup-complete` | `{ providerId, state }` when `setup_status.ok === true`. |

The Teams-specific events remain for backward compatibility:

| Event | Detail |
| --- | --- |
| `greentic-teams-setup-state` | `{ providerId, state }` after the configured state endpoint loads. |
| `greentic-teams-setup-result` | `{ providerId, result }` after setup/config actions. |
| `greentic-teams-setup-action-start` | `{ providerId, action, before }` when the visible action starts. |
| `greentic-teams-setup-action-complete` | `{ providerId, action, state }` when the action advances setup. |
| `greentic-teams-setup-action-timeout` | `{ providerId, action, error }` when no next step appears before timeout. |
| `greentic-teams-setup-device-login` | `{ providerId, login }` when a device-code login is needed. |
| `greentic-teams-setup-error` | `{ providerId, error }` when an API call fails. |
| `greentic-teams-setup-copy-code` | `{ providerId, code }` when the admin copies an OAuth code. |
| `greentic-teams-setup-complete` | `{ providerId, state }` when `setup_status.ok === true`. |

## Expected State Shape

The component is tolerant of missing fields, but it expects setup status under
`setup_status`, Teams app links under `teams_app`, and persisted config under
`values.config`.

## Generic Descriptor

The generated pack declares:

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
    "state-path": "/v1/messaging/setup/messaging-teams/{tenant}",
    "next-path": "/v1/messaging/setup/messaging-teams/{tenant}/next"
  },
  "completion": {
    "event": "greentic-provider-setup-complete",
    "state_event": "greentic-provider-setup-state",
    "state_path": "setup_status.ok",
    "equals": true
  }
}
```

A generic setup host should import `module_url` or resolve `module_asset`
through the static asset route, create `tag_name`, apply the templated
attributes, and mark the provider setup as done when either the completion event
fires or the latest state event has `setup_status.ok === true`.

## Setup Backend Contract

The web component is UI only. The real setup backend is `greentic-setup`
(`gtc setup`) serving the routes declared in `assets/setup/backend-contract.json`
and exposed through the `greentic.setup.backend-contract.v1` pack extension.

The contract declares `actions_schema_id: greentic.setup.actions.v1`. Each
`required_order` step has a matching action with a generic `executor.kind` so
`/next` can run without Teams-specific code in the setup host:

| Step | Executor kind |
| --- | --- |
| `graph_admin_consent` | `oauth_device_code` |
| `bot_app_identity` | `microsoft_graph_application` |
| `bot_framework_endpoint_registration` | `bot_framework_registration` |
| `teams_app_publish` | `microsoft_graph_teams_app_catalog_publish` |
| `teams_app_user_install` | `microsoft_graph_teams_app_user_install` |
| `first_bot_framework_post` | `runtime_observation` |

The Graph OAuth action provides `client_id_default` for the Microsoft Graph
Command Line Tools public client. Admins can still override it with
`graph_setup_client_id`, but the wizard must not require that value for the
default path.

The Bot Framework registration action requires:

- `public_base_url`: the public runtime base URL that Teams can reach.
- `bot_framework_registration_url`: the Greentic Bot Service endpoint that
  accepts `{ bot_app_id, bot_app_password, messaging_endpoint, channel }` and
  registers the Teams bot app with the Bot Framework-compatible service.

`greentic-setup` must implement those generic executor kinds and:

- Ignore browser-submitted `oauth_kind`, `oauth_device_code`, `oauth_user_code`,
  `graph_access_token`, `azure_management_access_token`, and `bot_access_token`.
- Persist OAuth/device-code state server side and only mutate it from
  `/oauth/{kind}/start` and `/oauth/{kind}/complete`.
- Register or update the Bot Framework-compatible endpoint for the bot app id
  before allowing Teams app publishing, Add to Teams, or Open bot chat.
- Treat manual Teams installation as an intermediate state only. Setup is not
  complete until `greentic-start` receives a Teams Bot Framework POST and
  persists that proof for setup status.

`greentic-start` (`gtc start`) should then focus on the configured provider:
Teams ingress to Greentic and Greentic egress to Teams.

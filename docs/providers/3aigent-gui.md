# 3AIgent GUI (`messaging.3aigent-gui`)

The WebChat GUI shipped as the 3AIgent product: the `3aigent` skin by default and
OAuth login enabled by default.

- Pack: `messaging-3aigent-gui`
- Component: `messaging-provider-3aigent-gui`
- Render tier: TierA (native Adaptive Cards), inherited from WebChat

## Relationship to messaging-webchat-gui

An alternative, never an addition. Both packs claim `/v1/web/webchat/{tenant}` and
`/v1/messaging/webchat/{tenant}`; a bundle carrying both fails static-route
validation in `greentic-start`, which is the intended behavior.

The component source-includes `gui_core.rs`, `config.rs` and `directline/` from
`messaging-provider-webchat-gui` and overrides exactly four constants:

| constant | webchat-gui | 3aigent-gui |
| --- | --- | --- |
| `PROVIDER_ID` | `messaging-provider-webchat-gui` | `messaging-provider-3aigent-gui` |
| `PROVIDER_TYPE` | `messaging.webchat-gui` | `messaging.3aigent-gui` |
| `DEFAULT_SKIN` | `default` | `3aigent` |
| `DEFAULT_OAUTH_ENABLED` | `false` | `true` |

## Setup

`oauth_enabled` defaults to `true`. Providers stay disabled until an administrator
supplies a client ID and secret in the setup wizard. Until then the SPA renders
"No sign-in provider configured" instead of an empty login screen.

**Custom OIDC has a wizard gap.** `assets/setup.yaml` has no
`oauth_custom_client_secret` question, and `compose_oauth_providers` in
`components/messaging-provider-webchat-gui/src/gui_core.rs` never attaches a
`client_secret` for the `custom` provider — while the `google`, `microsoft` and
`github` branches all do. A custom OIDC provider configured purely through the
setup wizard ends up with no client secret, and its token exchange fails.

There is a working route that does not depend on the wizard:
`ConfigAwareSecretStore::get` (in
`components/messaging-provider-webchat/src/directline/host.rs`) reads
`{key}_b64` (base64) from the injected config first, then falls back to the
host secrets store by plain key name. Supplying `oauth_custom_client_secret` as
a deployment secret or an environment-injected value reaches
`handle_oauth_token_exchange` correctly, even though the wizard cannot set it.

**Redirect URI must match exactly.** `initiateOAuthFlow` builds `redirect_uri`
as `window.location.href.split('?')[0]` — the live page URL minus its query
string. Whatever is registered at the identity provider must match that
exactly, including the trailing slash and the tenant path segment. A mismatch
here is the single most common cause of a failed OAuth demo.

The pack owns `assets/webchat-gui/config/tenants/default.json`. Two properties in
it are load-bearing:

- `"skin": "3aigent"` must be literal — `greentic-start` does not synthesize the
  tenant config for the tenant id `default`, it serves this file verbatim.
- Each entry in `auth.providers` must carry an `id` matching the wizard answer key
  `oauth_enable_<id>`. `greentic-start` only flips `enabled` on providers already
  declared here; an empty array means no provider can ever be turned on.

## Publishing

```bash
./scripts/publish_provider.sh 3aigent-gui
```

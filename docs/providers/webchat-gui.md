# WebChat GUI Provider

WebChat GUI is the browser face of Greentic messaging: a polished chat surface
you can host as a full page, drop into an existing site, or hand to a coding
agent as a predictable integration target.

It is built for two audiences at once:

- **Non-technical web developers** get copy-paste HTML, named skins, and a local
  preview command.
- **Coding agents** get stable files, attributes, modes, and focused validation
  commands.

## What You Can Build

Use WebChat GUI when you want:

- A full hosted chat page at `/v1/web/webchat/{tenant}/`.
- A `<greentic-webchat>` element that works in plain HTML, React, Vue, Astro,
  Svelte, WordPress, or any page that can load a module script.
- A brandable chat skin such as `default` or `3aigent`.
- A safe iframe embed for fast drop-in installs.
- A native embed for teams that want their website CSS to shape the chat.
- A popup/launcher experience for help buttons and support widgets.
- Native Adaptive Card rendering through BotFramework WebChat.

## The Integration Model

The public API is the Web Component:

```html
<script type="module" src="https://chat.example.com/v1/web/webchat/default/embed.js"></script>

<greentic-webchat
  tenant="default"
  mode="inline"
  render="native">
</greentic-webchat>
```

Choose the experience with two plain words:

| Attribute | Good Values | What It Means |
| --- | --- | --- |
| `mode` | `inline`, `popup`, `launcher` | Where the chat lives in the host page. |
| `render` | `iframe`, `native` | Whether the chat is isolated or styled by the host page. |

Recommended defaults:

| Use Case | Recommended Setup |
| --- | --- |
| Full-page chat link | Open `/v1/web/webchat/{tenant}/` directly. No iframe needed. |
| Drop-in support widget | `<greentic-webchat mode="launcher" render="iframe">` |
| Popup opened by a site button | `<greentic-webchat mode="popup" render="iframe">` |
| Inline app panel, safest install | `<greentic-webchat mode="inline" render="iframe">` |
| Inline app panel, host-styled | `<greentic-webchat mode="inline" render="native">` |

`iframe` is the durable, isolated choice. `native` is the expressive choice: it
loads the WebChat app directly into the host page so site CSS and developer
tooling can see and style the surface.

## Setup Inputs

In `gtc setup`, the important choices are:

- **Public base URL**: the public origin that serves the GUI.
- **Route/channel name**: usually `webchat`.
- **Delivery mode**: usually local Direct Line-style delivery.
- **JWT signing key secret**: used for Direct Line token signing.
- **Presentation mode**:
  - `standalone` for a hosted full-page chat.
  - `embed_webcomponent` for `<greentic-webchat>`.
- **Skin**: visual theme folder, such as `default` or `3aigent`.
- **Navigation links**: standalone/full-page only.

Keep this distinction crisp:

- `presentation_mode` chooses hosted page versus web component.
- `mode` and `render` choose how the web component behaves in a web page.
- `skin` only chooses the visual theme.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `jwt_signing_key` | Yes | Signs and verifies Direct Line tokens. |

Do not place secret values in HTML. The Web Component talks to public token
endpoints; the provider and operator keep credentials server-side.

## Local Preview

Preview the packaged GUI with a mocked local backend:

```bash
scripts/test_webchat_gui.sh default
scripts/test_webchat_gui.sh 3aigent
```

Preview every embed style side by side:

```bash
scripts/test_webchat_gui.sh 3aigent --embedded
```

The embedded preview shows:

- inline iframe web component
- inline native web component
- popup web component
- direct full-page native URL

Preview the login page for a specific skin:

```bash
scripts/test_webchat_gui.sh 3aigent --login
```

Disable the typing area to verify read-only or button-only flows:

```bash
scripts/test_webchat_gui.sh 3aigent --embedded --no-text-input
```

Add standalone top-bar links:

```bash
scripts/test_webchat_gui.sh 3aigent --demo-links
scripts/test_webchat_gui.sh 3aigent --nav-link 'M1|Playground|https://example.com'
```

## Build And Release

Focused local build:

```bash
scripts/build_providers.sh webchat-gui
```

Change only the WebChat GUI provider version, validate, and build:

```bash
scripts/change_provider_version.sh webchat-gui 0.4.99
```

Publish one provider after the branch is pushed:

```bash
scripts/publish_provider.sh webchat-gui 0.4.99
```

If you rebuilt the browser SPA in `greentic-webchat`, import the fresh assets
before building the provider pack:

```bash
GREENTIC_WEBCHAT_SITE_DIR=/projects/ai/greentic-ng/greentic-webchat/site/app \
  tools/import_webchat_gui_assets.sh

scripts/build_providers.sh webchat-gui
```

## Owned Files

- `components/messaging-provider-webchat-gui/`
- `packs/messaging-webchat-gui/`
- `packs/messaging-webchat-gui/assets/webchat-gui/`
- `tools/import_webchat_gui_assets.sh`
- `scripts/test_webchat_gui.sh`
- `docs/guides/webchat-gui-embed-webcomponent.md`

## Focused Checks

```bash
cargo test -p messaging-provider-webchat-gui
cargo test -p greentic-messaging-provider-common webchat_gui_config_schema_declares_presentation_mode
scripts/build_providers.sh webchat-gui
scripts/test_webchat_gui.sh 3aigent --embedded
```

## Agent Notes

- Do not use `skin` as behavior. `skin` is visual theme only.
- Keep `standalone` and `embed_webcomponent` setup behavior in the provider.
- Keep `mode` and `render` behavior in `embed.js`.
- `render="native"` intentionally allows host-page CSS to participate.
- `render="iframe"` should stay the safest default for untrusted or unknown host
  pages.

See the [WebChat GUI Web Component guide](../guides/webchat-gui-embed-webcomponent.md)
for copy-paste HTML, React, Vue, security, and troubleshooting examples.

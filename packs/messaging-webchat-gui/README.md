# Messaging WebChat GUI Pack

Hosted WebChat messaging provider combining Direct Line backend behavior with packaged `greentic-webchat` assets.

## Pack ID
- `messaging-webchat-gui`

## Providers
- `messaging.webchat-gui`

## Components
- `messaging-provider-webchat-gui` — provider-core WASM for Direct Line backend behavior

## Hosted routes
- GUI base: `/v1/web/webchat/{tenant}`
- Embed script: `/v1/web/webchat/{tenant}/embed.js`
- Backend base: `/v1/messaging/webchat/{tenant}/...`

## Assets
- `assets/webchat-gui/...` — packaged static frontend

## Presentation modes

- `standalone` hosts the full WebChat GUI page, including its own page shell/header/top navigation.
- `embed_webcomponent` exposes a framework-independent `<greentic-webchat>` Web Component for existing customer websites.

`skin` remains the visual theme folder, for example `default` or `3aigent`. Use `presentation_mode`, not `skin`, to select embedded behavior. In embedded mode, `nav_links` is not required because the host website owns navigation.

See [WebChat GUI Embed Web Component](../../docs/guides/webchat-gui-embed-webcomponent.md) for HTML, React, Vue, security, and troubleshooting guidance.

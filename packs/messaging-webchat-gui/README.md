# Messaging WebChat GUI Pack

This pack ships the Greentic browser chat experience: hosted page, embeddable
Web Component, skins, setup metadata, and the provider component that connects
the GUI to the WebChat backend.

## Pack ID

- `messaging-webchat-gui`

## Provider

- `messaging.webchat-gui`

## What It Serves

- Full-page GUI: `/v1/web/webchat/{tenant}/`
- Web Component script: `/v1/web/webchat/{tenant}/embed.js`
- WebChat backend routes: `/v1/messaging/webchat/{tenant}/...`

## Embed Story

For a full-page experience, link directly to:

```text
/v1/web/webchat/default/
```

For an existing website, load the Web Component:

```html
<script type="module" src="/v1/web/webchat/default/embed.js"></script>

<greentic-webchat
  tenant="default"
  mode="inline"
  render="native">
</greentic-webchat>
```

Use `render="iframe"` for safe isolation and `render="native"` when the host
site should style the chat directly.

## Presentation Modes

- `standalone`: hosted page with Greentic shell, skin, and optional top-bar links.
- `embed_webcomponent`: customer site owns the page; Greentic provides
  `<greentic-webchat>`.

`skin` is only the visual theme folder, for example `default` or `3aigent`.
Use `presentation_mode`, `mode`, and `render` for behavior.

## Setup Defaults

- `mode` defaults to `websocket` so the hosted GUI uses the push-capable
  Direct Line transport unless a bundle explicitly opts into another mode.
- `jwt_signing_key` is generated during setup and persisted as a tenant secret.
  Operators should not be prompted to provide it manually for a normal setup.
- The pack declares `jwt_signing_key` as a generated tenant-wide runtime secret
  so hosts can seed it generically for existing bundles.

## Local Preview

```bash
scripts/test_webchat_gui.sh default
scripts/test_webchat_gui.sh 3aigent --embedded
scripts/test_webchat_gui.sh 3aigent --login
```

`--embedded` shows iframe, native, popup, and full-page modes together.
`--login` clears the local test auth session and opens the real full-page login
screen for the selected skin.

See [WebChat GUI Web Component](../../docs/guides/webchat-gui-embed-webcomponent.md)
for HTML, React, Vue, security, and troubleshooting guidance.

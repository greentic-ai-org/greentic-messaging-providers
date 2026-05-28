# WebChat GUI Web Component

Add Greentic chat to any website with one script and one custom element.

This guide is written for web developers who want a working embed quickly, and
for coding agents that need exact attributes and safe defaults.

## The 30-Second Embed

```html
<script type="module" src="https://chat.example.com/v1/web/webchat/default/embed.js"></script>

<greentic-webchat
  tenant="default"
  mode="launcher"
  render="iframe">
</greentic-webchat>
```

That gives you a floating chat launcher with iframe isolation. It is the safest
copy-paste option for most websites.

## Pick A Shape

`mode` controls where the chat appears.

| Mode | Use It For | Example |
| --- | --- | --- |
| `launcher` | A floating chat bubble. | Help desk, support widget, marketing site. |
| `popup` | A chat panel opened by your own button/modal. | Product dashboards and portals. |
| `inline` | Chat placed directly inside your page layout. | App panels, demos, training pages. |

`render` controls how the chat is mounted.

| Render | Best For | Tradeoff |
| --- | --- | --- |
| `iframe` | Reliable drop-in embeds. | Strong isolation; host CSS cannot easily style internals. |
| `native` | Deep website integration. | Host CSS can style the chat; host CSS can also break it. |

Recommended pairings:

```html
<!-- Safest support widget -->
<greentic-webchat tenant="default" mode="launcher" render="iframe"></greentic-webchat>

<!-- Inline, isolated -->
<greentic-webchat tenant="default" mode="inline" render="iframe"></greentic-webchat>

<!-- Inline, host-styled -->
<greentic-webchat tenant="default" mode="inline" render="native"></greentic-webchat>
```

For a full-page chat, skip the Web Component entirely and link directly:

```text
https://chat.example.com/v1/web/webchat/default/
```

## Plain HTML

```html
<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Customer portal</title>
    <script type="module" src="https://chat.example.com/v1/web/webchat/default/embed.js"></script>
    <style>
      .support-panel {
        width: min(720px, 100%);
        height: 680px;
      }
    </style>
  </head>
  <body>
    <h1>Support</h1>

    <section class="support-panel">
      <greentic-webchat
        tenant="default"
        mode="inline"
        render="native">
      </greentic-webchat>
    </section>
  </body>
</html>
```

## Popup With Your Own Button

```html
<script type="module" src="https://chat.example.com/v1/web/webchat/default/embed.js"></script>

<button id="open-chat" type="button">Ask the assistant</button>

<dialog id="chat-dialog">
  <button id="close-chat" type="button">Close</button>
  <greentic-webchat
    tenant="default"
    mode="popup"
    render="iframe">
  </greentic-webchat>
</dialog>

<script>
  const dialog = document.getElementById("chat-dialog");
  document.getElementById("open-chat").onclick = () => dialog.showModal();
  document.getElementById("close-chat").onclick = () => dialog.close();
</script>
```

## Read-Only Or Button-Only Chat

Hide the text input when the conversation should be driven by Adaptive Card
buttons, menu choices, or demo content:

```html
<greentic-webchat
  tenant="default"
  mode="inline"
  render="native"
  text-input="false">
</greentic-webchat>
```

Equivalent:

```html
<greentic-webchat tenant="default" disable-text-input></greentic-webchat>
```

## React

```tsx
import { useEffect } from "react";

export function SupportChat() {
  useEffect(() => {
    import("https://chat.example.com/v1/web/webchat/default/embed.js");
  }, []);

  return (
    <div style={{ height: 680 }}>
      <greentic-webchat
        tenant="default"
        mode="inline"
        render="native"
      />
    </div>
  );
}
```

TypeScript JSX declaration:

```tsx
declare namespace JSX {
  interface IntrinsicElements {
    "greentic-webchat": React.DetailedHTMLProps<
      React.HTMLAttributes<HTMLElement>,
      HTMLElement
    > & {
      tenant?: string;
      mode?: "inline" | "popup" | "launcher";
      render?: "iframe" | "native";
      skin?: string;
      locale?: string;
      "public-base-url"?: string;
      "api-base"?: string;
      "text-input"?: "true" | "false";
      "disable-text-input"?: string;
    };
  }
}
```

## Vue

```vue
<script setup>
import { onMounted } from "vue";

onMounted(async () => {
  await import("https://chat.example.com/v1/web/webchat/default/embed.js");
});
</script>

<template>
  <div style="height: 680px">
    <greentic-webchat
      tenant="default"
      mode="inline"
      render="iframe"
    />
  </div>
</template>
```

## Attributes

| Attribute | Purpose |
| --- | --- |
| `tenant` | Tenant route segment, such as `default` or `3aigent`. |
| `mode` | `launcher`, `popup`, or `inline`. |
| `render` | `iframe` or `native`. |
| `public-base-url` | Public Greentic origin. Defaults to the origin serving `embed.js`. |
| `api-base` | Optional backend base URL override. |
| `skin` | Optional visual skin override. Usually the tenant config decides this. |
| `locale` | Locale hint, such as `en`, `nl`, or `fr`. |
| `text-input` | `true` or `false`. Defaults to `true`. |
| `disable-text-input` | Boolean shortcut that hides the text input. |
| `open` | Opens launcher/docked chat when present. |
| `title` | Accessible title for the chat surface. |

## Events

All events bubble and cross the component boundary:

- `greentic-webchat-ready`
- `greentic-webchat-open`
- `greentic-webchat-close`
- `greentic-webchat-error`

Example:

```js
document.addEventListener("greentic-webchat-error", (event) => {
  console.error("Chat failed", event.detail);
});
```

## Local Preview

Preview the current pack with a mocked backend:

```bash
scripts/test_webchat_gui.sh default
scripts/test_webchat_gui.sh 3aigent
```

Preview iframe, native, popup, and full-page modes together:

```bash
scripts/test_webchat_gui.sh 3aigent --embedded
```

Add demo navigation links for standalone/full-page skins:

```bash
scripts/test_webchat_gui.sh 3aigent --demo-links
scripts/test_webchat_gui.sh 3aigent --nav-link 'M1|Playground|https://example.com'
```

## Setup In Greentic

In `gtc setup`, choose:

- `presentation_mode=standalone` for a hosted full-page chat.
- `presentation_mode=embed_webcomponent` for customer-site embeds.

Then choose the `skin`, such as `default` or `3aigent`.

`nav_links` belongs to standalone/full-page pages. Embedded pages usually use
the host website's own navigation.

Example setup answers:

```json
{
  "setup_answers": {
    "messaging-webchat-gui": {
      "public_base_url": "https://chat.example.com",
      "mode": "local_queue",
      "route": "webchat",
      "jwt_signing_key": "stored-secret-name",
      "presentation_mode": "embed_webcomponent",
      "skin": "default"
    }
  }
}
```

## Security

- Do not put secrets in HTML.
- Use HTTPS in production.
- Add the Greentic deployment origin to `script-src` and `connect-src` in your
  Content Security Policy.
- For `render="native"`, remember that host CSS and Greentic CSS share the same
  page. Treat it like any other app component.
- For unknown host pages, prefer `render="iframe"`.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `embed.js` returns 404 | Confirm `/v1/web/webchat/{tenant}/embed.js` is served by the pack. |
| Custom element is not defined | Use `<script type="module" ...>`. Check CSP. |
| Wrong skin appears | Confirm the tenant config points at the expected skin folder. |
| Text input is hidden | Check `text-input`, `disable-text-input`, and tenant style options. |
| Native mode looks different | Host CSS is participating. Use iframe mode for isolation. |
| Messages do not send | Check the Direct Line/token backend and provider setup. |
| OAuth/token errors | Check WebChat GUI provider setup and server logs. |

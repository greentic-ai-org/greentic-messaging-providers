# WebChat GUI Embed Web Component

The `messaging-webchat-gui` pack supports two presentation modes:

- `standalone`: Greentic hosts a full WebChat GUI page, including its own page shell, header, and top navigation links.
- `embed_webcomponent`: Greentic serves a framework-independent Web Component that a customer can mount inside an existing website. The customer website owns the header, navigation, layout, and surrounding page.

`skin` remains the visual theme folder, such as `default` or `3aigent`. Do not use `skin` to select embed behavior.

## Setup

In `gtc setup`, choose `embed_webcomponent` for `presentation_mode`. Keep selecting `skin` for the visual theme. `nav_links` is not needed in embedded mode and is not prompted when `presentation_mode` is `embed_webcomponent`.

Example answers:

```json
{
  "setup_answers": {
    "messaging-webchat-gui": {
      "public_base_url": "https://chat.example.com",
      "mode": "local_queue",
      "route": "webchat",
      "jwt_signing_key": "change-me",
      "presentation_mode": "embed_webcomponent",
      "skin": "default"
    }
  }
}
```

Existing answer files without `presentation_mode` continue to behave as `standalone`.

## Plain HTML

```html
<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Existing website</title>
    <script
      type="module"
      src="https://chat.example.com/v1/web/webchat/demo/embed.js">
    </script>
  </head>
  <body>
    <h1>My existing website</h1>

    <greentic-webchat
      tenant="demo"
      api-base="https://chat.example.com/v1/messaging/webchat/demo"
      skin="default"
      launcher="true">
    </greentic-webchat>
  </body>
</html>
```

## React

```tsx
import { useEffect } from "react";

export function SupportChat() {
  useEffect(() => {
    import("https://chat.example.com/v1/web/webchat/demo/embed.js");
  }, []);

  return (
    <greentic-webchat
      tenant="demo"
      api-base="https://chat.example.com/v1/messaging/webchat/demo"
      skin="default"
      launcher="true"
    />
  );
}
```

For TypeScript JSX, add a declaration:

```tsx
declare namespace JSX {
  interface IntrinsicElements {
    "greentic-webchat": React.DetailedHTMLProps<
      React.HTMLAttributes<HTMLElement>,
      HTMLElement
    > & {
      tenant?: string;
      "api-base"?: string;
      skin?: string;
      launcher?: string;
    };
  }
}
```

## Vue

```vue
<script setup>
import { onMounted } from "vue";

onMounted(async () => {
  await import("https://chat.example.com/v1/web/webchat/demo/embed.js");
});
</script>

<template>
  <greentic-webchat
    tenant="demo"
    api-base="https://chat.example.com/v1/messaging/webchat/demo"
    skin="default"
    launcher="true"
  />
</template>
```

## Angular And Other Loaders

For Angular, Astro, Svelte, and other frameworks, load the module script in the host page or dynamically import it from a component:

```html
<script type="module" src="https://chat.example.com/v1/web/webchat/demo/embed.js"></script>
```

Then render `<greentic-webchat>` wherever the host layout should show chat.

## Attributes

- `tenant`: Greentic tenant segment used by the hosted WebChat route.
- `api-base`: Provider backend base URL, usually `/v1/messaging/webchat/{tenant}`.
- `public-base-url`: Greentic public origin. Defaults to the origin serving `embed.js`.
- `skin`: Visual theme folder, such as `default` or `3aigent`.
- `launcher`: `true` shows a floating launcher button. `false` renders inline.
- `open`: Opens the docked chat when present.
- `locale`: Locale hint passed to the WebChat GUI.
- `title`: Accessible title for the iframe and launcher.

## Events

- `greentic-webchat-ready`
- `greentic-webchat-open`
- `greentic-webchat-close`
- `greentic-webchat-error`

All events bubble and are composed, so host pages can listen outside the component shadow root.

## Security

Do not put secrets in HTML. The Web Component should use only public/token endpoints. Configure OAuth and Direct Line token handling through provider setup.

Use HTTPS in production. If the customer website and Greentic deployment use different origins, configure CORS for the WebChat backend and include the Greentic origin in the host site CSP `script-src` and `connect-src`. If allowed parent origins are implemented by the deployment, restrict them to the known customer site origins.

## Troubleshooting

- `embed.js` returns 404: verify the pack includes `assets/webchat-gui/embed.js` and the static route serves `/v1/web/webchat/{tenant}/embed.js`.
- Custom element not defined: make sure the script tag uses `type="module"` and is not blocked by CSP.
- CSP blocks script: add the Greentic deployment origin to `script-src`.
- CORS blocks API calls: allow the customer website origin for the WebChat backend.
- Wrong tenant: confirm the `tenant` attribute matches the deployed tenant path.
- Token/auth failures: check the WebChat GUI provider setup and token endpoint configuration.
- Skin not found: confirm the theme folder exists under `assets/webchat-gui/skins`.
- Chat opens but messages do not send: confirm `api-base` points at `/v1/messaging/webchat/{tenant}` and the provider backend is healthy.

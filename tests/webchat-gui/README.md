# WebChat GUI Playwright Tests

This suite exercises the shipped `messaging-webchat-gui` browser assets as a customer would use them: full-screen WebChat, native and iframe embeds, popup/launcher modes, skins, navigation, and login gating.

## Run Locally

Install dependencies once:

```bash
npm install
npx playwright install --with-deps chromium
```

Run the standard headless suite:

```bash
npm run test:webchat-gui
```

Run headed/debug:

```bash
npm run test:webchat-gui:headed
```

Build the provider pack inputs first, then run the suite:

```bash
scripts/test_webchat_gui_playwright.sh
```

Pass `--no-build` to skip rebuilding when iterating on tests.

## Visual Snapshots

Visual snapshots are opt-in to keep default CI stable:

```bash
npm run test:webchat-gui:update-snapshots
WEBCHAT_GUI_VISUAL=1 npm run test:webchat-gui -- --grep @visual
```

Covered visual states:

- Full-screen default skin
- Full-screen `3aigent` skin
- Native popup closed
- Native popup opened
- Iframe inline
- Login page

## Test Matrix

The suite avoids a full Cartesian product while covering each behavior where it matters most.

| Area | Coverage |
| --- | --- |
| Full-screen | default and `3aigent`; nav enabled/disabled; login enabled/disabled; desktop and mobile |
| Embedded native | inline and launcher/popup behavior; keyboard open/close; default and `3aigent` |
| Embedded iframe | inline and launcher; default and `3aigent`; login enabled in iframe |
| Login | anonymous start, login gate, dummy success, callback error state |
| Accessibility/responsive | launcher keyboard focus, visible input, labelled input, mobile viewport, popup in viewport |
| Assets | broken image checks and local-only mock assets |

## Mock Backend

`fixtures/server.mjs` serves the real assets from:

```text
packs/messaging-webchat-gui/assets/webchat-gui
```

It also provides deterministic local endpoints:

- `/v1/messaging/webchat/{tenant}/auth/config`
- `/v1/messaging/webchat/{tenant}/token`
- `/v1/messaging/webchat/{tenant}/v3/directline/...`
- `/mock-api/messages`

The production app normally loads Bot Framework WebChat from the public CDN. Tests intercept that request and fulfill it with `fixtures/mock-webchat.js`, a tiny deterministic `window.WebChat` implementation that renders an accessible transcript, input, and send button. This keeps tests offline and stable.

## Adding Coverage

Add new scenarios under `specs/` and reusable interactions in `pages/webchatGuiPage.ts`. Prefer visible text, ARIA roles, or `data-testid` on fixture host pages. For new skins, add a tenant alias in `fixtures/server.mjs` and include it in the matrix deliberately rather than expanding every combination.

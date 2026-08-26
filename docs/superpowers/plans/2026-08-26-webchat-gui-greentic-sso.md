# WebChat GUI Greentic SSO Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Greentic SSO the first, default-enabled login option in the WebChat GUI by importing the `@greentic/sso` SDK, and bind the DirectLine chat token to the verified SSO identity.

**Architecture:** The SDK is pulled in as a git-pinned npm dependency and bundled by esbuild into a committed IIFE asset. `runtime-bootstrap.js` gains a `type: "greentic"` branch that drives the SDK; every other provider type keeps the existing redirect PKCE path, which also serves as the `popup_blocked` fallback. The Rust side gains an ES256 access-token verifier so `/token` can mint an identity-bound DirectLine token.

**Tech Stack:** Rust 1.95 / edition 2024 (`wasm32-wasip2`), vanilla ES5-style browser JS, esbuild, Playwright, `p256` (RustCrypto).

**Spec:** `docs/superpowers/specs/2026-08-26-webchat-gui-greentic-sso-design.md`

## Global Constraints

- Rust toolchain **1.95.0**, edition 2024. No `unwrap()` / `panic!()` in production paths.
- Every `.rs` file stays under **500 lines**; split a module rather than growing one past that.
- `components/messaging-provider-webchat/src/describe.rs` and `src/ops/` are shared with `messaging-provider-webchat-gui` through `#[path]`. Every change there lands in **both** providers and **both** packs. This is intended (decision D3).
- Browser assets are classic scripts, not modules. `runtime-bootstrap.js` uses `var` and `function`, no arrow functions, no `const`/`let`. Match that style.
- New browser assets go at the **top level** of `packs/messaging-webchat-gui/assets/webchat-gui/`. The subdirectories `assets/`, `config/`, `i18n/` and `js/` are `rsync --delete`d by `tools/import_webchat_gui_assets.sh`.
- `index.html` is **generated**. Script tags are added to the heredoc in `tools/import_webchat_gui_assets.sh`, never to `index.html` directly.
- Comments: default to none. One short line only when the *why* is non-obvious. Never describe *what* the code does.
- Conventional commits. **No Claude co-author or "Generated with" trailers.**
- CI installing the git dependency must not use `--omit=dev` or `--ignore-scripts`; the SDK's `dist/` is produced by its `prepare` script.
- SDK pin: `github:greenticai/greentic-webchat-sdk#2696c1083f90886f82b65aab4573d1b7458925c0`. The repo has no tags; the SHA is the pin.

---

# PR 1 — SDK login (layers A, B, C)

At the end of PR 1, Greentic SSO is the first login button and completes a real login. The DirectLine token stays anonymous, exactly as today.

---

### Task 1: Vendor the SDK and build the IIFE bundle

The SDK's `/webchat` entry has no IIFE build — `tsup.config.ts` gives it esm+cjs only, and only the core entry sets `globalName: "GreenticSso"`. So a local entry that re-exports both is required.

**Files:**
- Modify: `package.json`
- Create: `tools/webchat-sso/entry.js`
- Create: `tools/build_webchat_sso_bundle.sh`
- Create: `packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js` (build output, committed)
- Modify: `.github/workflows/webchat-gui-playwright.yml`
- Modify: `.gitignore` (ensure `node_modules/` is ignored)

**Interfaces:**
- Produces: global `window.GreenticSso` with `createGreenticSso`, `createGreenticWebchatSso`, `mintChatToken`, `completeCallbackFromPopup`, `GreenticSsoError`. Tasks 2, 3 and 13 consume it.

- [ ] **Step 1: Add the dependency and the build script**

In `package.json`, add to `devDependencies` and add a new `dependencies` block:

```json
  "dependencies": {
    "@greentic/sso": "github:greenticai/greentic-webchat-sdk#2696c1083f90886f82b65aab4573d1b7458925c0"
  },
  "devDependencies": {
    "@playwright/test": "^1.56.1",
    "esbuild": "^0.25.0"
  }
```

And add to `scripts`:

```json
    "build:sso-bundle": "bash tools/build_webchat_sso_bundle.sh"
```

- [ ] **Step 2: Write the bundle entry**

Create `tools/webchat-sso/entry.js`:

```js
export { createGreenticSso, completeCallbackFromPopup, GreenticSsoError } from "@greentic/sso";
export { createGreenticWebchatSso, mintChatToken } from "@greentic/sso/webchat";
```

- [ ] **Step 3: Write the build script**

Create `tools/build_webchat_sso_bundle.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT_DIR}/packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js"

npx --no-install esbuild "${ROOT_DIR}/tools/webchat-sso/entry.js" \
  --bundle \
  --format=iife \
  --global-name=GreenticSso \
  --target=es2017 \
  --legal-comments=none \
  --outfile="${OUT}"

echo "built ${OUT}"
```

Make it executable: `chmod +x tools/build_webchat_sso_bundle.sh`

- [ ] **Step 4: Install and build**

Run:
```bash
npm install
npm run build:sso-bundle
```
Expected: `packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js` exists.

- [ ] **Step 5: Verify the global surface**

Run:
```bash
node -e '
  const fs = require("fs");
  const src = fs.readFileSync("packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js", "utf8");
  const sandbox = { window: {} };
  new Function("window", src + "; window.GreenticSso = GreenticSso;")(sandbox.window);
  const api = sandbox.window.GreenticSso;
  const need = ["createGreenticSso","createGreenticWebchatSso","mintChatToken","completeCallbackFromPopup","GreenticSsoError"];
  const missing = need.filter(function (k) { return typeof api[k] === "undefined"; });
  if (missing.length) { console.error("missing:", missing); process.exit(1); }
  console.log("ok: all exports present");
'
```
Expected: `ok: all exports present`

- [ ] **Step 6: Add the CI drift check**

In `.github/workflows/webchat-gui-playwright.yml`, after the existing `npm ci` (or `npm install`) step, add:

```yaml
      - name: Verify SSO bundle is up to date
        run: |
          npm run build:sso-bundle
          git diff --exit-code packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js
```

Confirm the workflow's install step does **not** pass `--omit=dev` or `--ignore-scripts`; if it does, remove those flags.

- [ ] **Step 7: Ignore node_modules**

Ensure `.gitignore` contains `node_modules/`. Add it if missing.

- [ ] **Step 8: Commit**

```bash
git add package.json package-lock.json .gitignore tools/webchat-sso/entry.js \
  tools/build_webchat_sso_bundle.sh \
  packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js \
  .github/workflows/webchat-gui-playwright.yml
git commit -m "build: bundle @greentic/sso into a webchat-gui IIFE asset"
```

---

### Task 2: SSO callback page

`completeCallbackFromPopup()` must run on a page served from the app's own origin. Its absolute URL is the `redirectUri` registered per tenant in the Greentic admin.

**Files:**
- Create: `packs/messaging-webchat-gui/assets/webchat-gui/sso-callback.html`
- Modify: `tools/import_webchat_gui_assets.sh` (index.html heredoc)
- Modify: `tests/webchat-gui/fixtures/server.mjs`
- Test: `tests/webchat-gui/specs/fullscreen.spec.ts`

**Interfaces:**
- Consumes: `window.GreenticSso.completeCallbackFromPopup` from Task 1.
- Produces: the path `sso-callback.html` relative to the GUI base, consumed by Task 3 when it builds `redirectUri`.

- [ ] **Step 1: Write the failing test**

Append to `tests/webchat-gui/specs/fullscreen.spec.ts`:

```ts
test('sso callback page loads the SDK and exposes the completer', async ({ page }) => {
  await page.goto('/v1/web/webchat/default-plain-anon/sso-callback.html');
  const hasCompleter = await page.evaluate(
    () => typeof (window as any).GreenticSso?.completeCallbackFromPopup === 'function',
  );
  expect(hasCompleter).toBe(true);
});
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `npm run test:webchat-gui -- --grep "sso callback page"`
Expected: FAIL — the route 404s.

- [ ] **Step 3: Create the callback page**

Create `packs/messaging-webchat-gui/assets/webchat-gui/sso-callback.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Signing you in…</title>
    <script src="./greentic-sso.js"></script>
  </head>
  <body>
    <p>Signing you in…</p>
    <script>
      if (window.GreenticSso && window.GreenticSso.completeCallbackFromPopup) {
        window.GreenticSso.completeCallbackFromPopup();
      }
    </script>
  </body>
</html>
```

- [ ] **Step 4: Serve it from the Playwright fixture**

In `tests/webchat-gui/fixtures/server.mjs`, the `/v1/web/webchat/{tenant}/…` static route already resolves through `appAssetPath`. Confirm `sso-callback.html` resolves; if the handler only special-cases `index.html`, add `sso-callback.html` alongside it.

- [ ] **Step 5: Add the script tag to the generated index.html**

In `tools/import_webchat_gui_assets.sh`, in the `cat > "${DEST_DIR}/index.html"` heredoc, add the SDK script **before** `runtime-bootstrap.js` so the global exists when the bootstrap runs:

```
    <script src="./greentic-sso.js"></script>
    <script src="./runtime-bootstrap.js"></script>
```

Then apply the same two lines to the committed `packs/messaging-webchat-gui/assets/webchat-gui/index.html` and `404.html`, since the import script only regenerates them when it runs.

- [ ] **Step 6: Run the test and confirm it passes**

Run: `npm run test:webchat-gui -- --grep "sso callback page"`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add packs/messaging-webchat-gui/assets/webchat-gui/sso-callback.html \
  packs/messaging-webchat-gui/assets/webchat-gui/index.html \
  packs/messaging-webchat-gui/assets/webchat-gui/404.html \
  tools/import_webchat_gui_assets.sh \
  tests/webchat-gui/fixtures/server.mjs \
  tests/webchat-gui/specs/fullscreen.spec.ts
git commit -m "feat(webchat-gui): add SSO popup callback page"
```

---

### Task 3: `greentic` provider branch in the auth gate

**Files:**
- Modify: `packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js` (`initiateOAuthFlow` at ~684, `applyAuthConfig` at ~995)
- Modify: `tests/webchat-gui/fixtures/server.mjs:151-162`
- Test: `tests/webchat-gui/specs/fullscreen.spec.ts`

**Interfaces:**
- Consumes: `window.GreenticSso.createGreenticWebchatSso`, `window.GreenticSso.GreenticSsoError` (Task 1); `sso-callback.html` (Task 2).
- Produces: `window.__GREENTIC_SSO_CLIENT__` — the live `GreenticWebchatSsoClient`, or `null`. Task 13 reads it to attach the bearer.
- Produces: `greenticSsoRedirectFallback(provider)` — starts the legacy redirect PKCE flow against the Greentic issuer.

- [ ] **Step 1: Extend the auth-config mock**

In `tests/webchat-gui/fixtures/server.mjs`, replace the `sendJson(res, 200, scenario.login ? … : …)` call in the `/auth/config` handler with:

```js
    if (scenario.ssoLogin) {
      sendJson(res, 200, {
        enabled: true,
        providers: [{
          id: 'greentic',
          label: 'Greentic SSO',
          type: 'greentic',
          enabled: true,
          issuer: `http://127.0.0.1:${port}/mock-idp`,
          client_id: 'webchat-gui',
        }],
      });
      return;
    }
    sendJson(res, 200, scenario.login
      ? { enabled: true, providers: [{ id: 'test-login', label: 'Test Login', type: 'dummy', enabled: true }] }
      : { enabled: false });
```

And in `tenantScenario(tenant)`, add `ssoLogin: tenant.includes('sso')` to the returned object. Place the `sso` check so it does not collide with the existing `login` substring check — a tenant named `default-plain-sso` must set `ssoLogin: true` and may also set `login: false`.

- [ ] **Step 2: Write the failing test**

Append to `tests/webchat-gui/specs/fullscreen.spec.ts`:

```ts
test('greentic sso provider renders first and drives the SDK', async ({ page }) => {
  await page.goto('/v1/web/webchat/default-plain-sso/');
  const overlay = page.locator('#greentic-oauth-overlay');
  await expect(overlay).toBeVisible();
  const firstButton = overlay.locator('button').first();
  await expect(firstButton).toHaveText(/greentic sso/i);

  const built = await page.evaluate(() => {
    const cfg = (window as any).__OAUTH_CONFIG__;
    return cfg && cfg.providers && cfg.providers[0] && cfg.providers[0].type;
  });
  expect(built).toBe('greentic');
});

test('popup_blocked falls back to the redirect flow', async ({ page }) => {
  await page.addInitScript(() => {
    (window as any).__FORCE_POPUP_BLOCKED__ = true;
  });
  const navigations: string[] = [];
  page.on('framenavigated', (f) => navigations.push(f.url()));
  await page.goto('/v1/web/webchat/default-plain-sso/');
  await page.locator('#greentic-oauth-overlay button').first().click();
  await expect
    .poll(() => navigations.some((u) => u.includes('/mock-idp/oauth/authorize')))
    .toBe(true);
});
```

- [ ] **Step 3: Run and confirm both fail**

Run: `npm run test:webchat-gui -- --grep "greentic sso provider|popup_blocked"`
Expected: FAIL — no `greentic` branch exists.

- [ ] **Step 4: Normalise the new provider fields**

In `runtime-bootstrap.js`, inside `applyAuthConfig`'s `.map(function (p) { … })`, add two fields to the returned object, after `response_type`:

```js
                response_type: p.response_type || p.responseType || 'code',
                issuer: p.issuer,
                chat_api_base: p.chat_api_base || p.chatApiBase
```

- [ ] **Step 5: Add the SSO helpers**

In `runtime-bootstrap.js`, immediately above `function initiateOAuthFlow(provider) {`, insert:

```js
  window.__GREENTIC_SSO_CLIENT__ = null;

  function greenticSsoRedirectUri() {
    return guiBase + 'sso-callback.html';
  }

  function greenticSsoIssuer(provider) {
    return provider.issuer || ('https://' + tenant + '.greentic-id.com');
  }

  function buildGreenticSsoClient(provider) {
    return window.GreenticSso.createGreenticWebchatSso({
      tenant: tenant,
      issuer: greenticSsoIssuer(provider),
      clientId: provider.client_id || 'webchat-gui',
      redirectUri: greenticSsoRedirectUri(),
      chatApiBase: provider.chat_api_base || backendBase(tenant),
      // Memory-only sessions die on reload, forcing a fresh popup per page load.
      persist: true
    });
  }

  function restoreGreenticSsoClient() {
    var provider = null;
    try {
      var raw = sessionStorage.getItem(oauthStorageKey('provider'));
      provider = raw ? JSON.parse(raw) : null;
    } catch (_) {}
    if (!provider || provider.type !== 'greentic') return null;
    if (!window.GreenticSso || !window.GreenticSso.createGreenticWebchatSso) return null;
    var config = (window.__OAUTH_CONFIG__ && window.__OAUTH_CONFIG__.providers || [])
      .filter(function (p) { return p.type === 'greentic'; })[0];
    if (!config) return null;
    var client = buildGreenticSsoClient(config);
    return client.isAuthenticated && client.isAuthenticated() ? client : null;
  }

  // The SDK awaits the PKCE challenge before window.open, which breaks the
  // user-gesture chain on Safari and Firefox; the legacy redirect flow is the
  // only way those browsers can complete a login.
  function greenticSsoRedirectFallback(provider) {
    var issuer = greenticSsoIssuer(provider);
    initiateOAuthFlow({
      id: provider.id,
      label: provider.label,
      type: 'oidc',
      auth_url: issuer + '/oauth/authorize',
      token_url: issuer + '/oauth/token',
      client_id: provider.client_id || 'webchat-gui',
      scopes: provider.scope || 'openid profile email greentic.webchat'
    });
  }

  function initiateGreenticSso(provider) {
    if (!window.GreenticSso || !window.GreenticSso.createGreenticWebchatSso) {
      greenticSsoRedirectFallback(provider);
      return;
    }
    if (window.__FORCE_POPUP_BLOCKED__) {
      greenticSsoRedirectFallback(provider);
      return;
    }
    var client = buildGreenticSsoClient(provider);
    window.__GREENTIC_SSO_CLIENT__ = client;
    client.login().then(function (identity) {
      saveOAuthSession('greentic-sso', 'greentic');
      try {
        if (identity.name) sessionStorage.setItem(oauthStorageKey('user_name'), identity.name);
        if (identity.email) sessionStorage.setItem(oauthStorageKey('user_email'), identity.email);
        sessionStorage.setItem(oauthStorageKey('provider'), JSON.stringify({ id: provider.id, type: 'greentic' }));
      } catch (_) {}
      removeOAuthOverlay();
      injectLogoutButton();
    }).catch(function (err) {
      window.__GREENTIC_SSO_CLIENT__ = null;
      if (err && err.code === 'popup_blocked') {
        greenticSsoRedirectFallback(provider);
        return;
      }
      showAuthError(uiT('login.failed', 'Authentication failed') + ': ' + ((err && err.message) || 'unknown'));
    });
  }
```

- [ ] **Step 6: Restore the client on reload**

In `applyAuthConfig`, replace the existing-session branch so a returning visitor
gets a working `getAccessToken()` rather than only a logout button:

```js
        var session = getOAuthSession();
        if (session) {
          console.log('[oauth] existing session found');
          window.__GREENTIC_SSO_CLIENT__ = restoreGreenticSsoClient();
          injectLogoutButton();
          return;
        }
```

- [ ] **Step 7: Dispatch on the provider type**

In `initiateOAuthFlow(provider)`, immediately after the opening brace and before the existing `if (provider.type === 'dummy')` block, insert:

```js
    if (provider.type === 'greentic') {
      initiateGreenticSso(provider);
      return;
    }
```

- [ ] **Step 8: Run and confirm both pass**

Run: `npm run test:webchat-gui -- --grep "greentic sso provider|popup_blocked"`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js \
  tests/webchat-gui/fixtures/server.mjs tests/webchat-gui/specs/fullscreen.spec.ts
git commit -m "feat(webchat-gui): drive Greentic SSO login through the SDK"
```

---

### Task 4: Scope the DirectLine cache to the identity

Today `directLineCacheKey()` omits the identity and `performLogout()` never clears the cache, so a token minted for one user is served from `localStorage` to the next user of the same browser. Identity-bound tokens (PR 2) would turn that into a cross-user session leak.

**Files:**
- Modify: `packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js` (`performLogout` ~759, `directLineCacheKey` ~1098)
- Modify: `crates/provider-common/tests/spec_lints.rs:960-967`
- Test: `tests/webchat-gui/specs/fullscreen.spec.ts`

**Interfaces:**
- Consumes: `getOAuthSession()`, `oauthStorageKey()` (existing).
- Produces: `directLineIdentityPart()` returning a stable string. `spec_lints.rs` asserts on its presence in the cache key.

- [ ] **Step 1: Write the failing Rust lint**

In `crates/provider-common/tests/spec_lints.rs`, inside `webchat_gui_direct_line_cache_is_scoped_and_401_safe`, add after the existing `stableCachePart` assertion block:

```rust
    assert!(
        runtime.contains("stableCachePart(directLineIdentityPart())"),
        "webchat-gui Direct Line cache key must include the authenticated identity"
    );
    assert!(
        runtime.contains("clearDirectLineCache();\n    clearOAuthSession();")
            || runtime.contains("clearOAuthSession();\n    clearAppAuthSession();\n    clearDirectLineCache();"),
        "webchat-gui logout must clear the Direct Line cache"
    );
```

- [ ] **Step 2: Write the failing Playwright test**

Append to `tests/webchat-gui/specs/fullscreen.spec.ts`:

```ts
test('logout clears the cached Direct Line token', async ({ page }) => {
  await page.goto('/v1/web/webchat/default-plain-login/');
  await page.getByRole('button', { name: /test login/i }).click();
  await expect.poll(async () =>
    page.evaluate(() => Object.keys(localStorage).some((k) => k.includes(':dl:token:'))),
  ).toBe(true);

  await page.evaluate(() => {
    const btn = document.getElementById('greentic-logout-btn') as HTMLButtonElement | null;
    btn?.click();
  });

  await expect.poll(async () =>
    page.evaluate(() => Object.keys(localStorage).some((k) => k.includes(':dl:token:'))),
  ).toBe(false);
});
```

- [ ] **Step 3: Run both and confirm they fail**

Run: `cargo test -p greentic-messaging-provider-common --test spec_lints webchat_gui_direct_line_cache`
Expected: FAIL — `directLineIdentityPart` not present.

Run: `npm run test:webchat-gui -- --grep "logout clears the cached"`
Expected: FAIL — the key survives logout.

- [ ] **Step 4: Add the identity cache part**

In `runtime-bootstrap.js`, immediately above `function directLineCacheKey(kind) {`, insert:

```js
  function directLineIdentityPart() {
    try {
      var sub = sessionStorage.getItem(oauthStorageKey('user_email'))
        || sessionStorage.getItem(oauthStorageKey('user_name'));
      if (sub) return sub;
      var session = getOAuthSession();
      if (session && session.token_handle) return session.token_handle;
    } catch (_) {}
    return 'anonymous';
  }
```

Then add it to the key array in `directLineCacheKey`, after `stableCachePart(flowId)`:

```js
      stableCachePart(flowId),
      stableCachePart(directLineIdentityPart())
```

- [ ] **Step 5: Make the cache keys lazy**

`TOKEN_CACHE_KEY`, `CONVERSATION_CACHE_KEY` and `DIRECT_LINE_AUTH_RETRY_KEY` are computed once at load, before any session exists. Replace the three `var … = directLineCacheKey('…');` lines with functions and update every reader:

```js
  function tokenCacheKey() { return directLineCacheKey('token'); }
  function conversationCacheKey() { return directLineCacheKey('conversation'); }
  function directLineAuthRetryKey() { return directLineCacheKey('auth-retry'); }
```

Replace each use of `TOKEN_CACHE_KEY` with `tokenCacheKey()`, `CONVERSATION_CACHE_KEY` with `conversationCacheKey()`, and `DIRECT_LINE_AUTH_RETRY_KEY` with `directLineAuthRetryKey()` throughout the file. `spec_lints.rs:981-987` asserts the literal `sessionStorage.setItem(DIRECT_LINE_AUTH_RETRY_KEY, '1')`, so update that assertion to `sessionStorage.setItem(directLineAuthRetryKey(), '1')` in the same commit.

- [ ] **Step 6: Clear the cache on logout**

Replace `performLogout`:

```js
  function performLogout() {
    clearOAuthSession();
    clearAppAuthSession();
    clearDirectLineCache();
    window.location.reload();
  }
```

`clearDirectLineCache` is defined later in the same IIFE; function declarations hoist, so this is safe.

- [ ] **Step 7: Run both and confirm they pass**

Run: `cargo test -p greentic-messaging-provider-common --test spec_lints webchat_gui_direct_line_cache`
Expected: PASS

Run: `npm run test:webchat-gui -- --grep "logout clears the cached"`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js \
  crates/provider-common/tests/spec_lints.rs tests/webchat-gui/specs/fullscreen.spec.ts
git commit -m "fix(webchat-gui): scope Direct Line cache to identity and clear it on logout"
```

---

### Task 5: Translate the login overlay

`i18n/en.json` already ships `login.title`, `login.subtitle`, `login.loginWith` and `login.noProviders`; only the React fallback page uses them. The overlay hardcodes English.

**Files:**
- Modify: `packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js` (`showLoginScreen` ~776)
- Test: `tests/webchat-gui/specs/fullscreen.spec.ts`
- Refresh: `tests/webchat-gui/specs/visual.spec.ts-snapshots/login-page-chromium-linux.png`

**Interfaces:**
- Consumes: `uiT(key, fallback)` (existing, `runtime-bootstrap.js:344`).

- [ ] **Step 1: Write the failing test**

Append to `tests/webchat-gui/specs/fullscreen.spec.ts`:

```ts
test('login overlay uses i18n keys rather than hardcoded English', async ({ page }) => {
  await page.goto('/v1/web/webchat/default-plain-login/');
  const usesKeys = await page.evaluate(() => {
    const card = document.querySelector('#greentic-oauth-overlay h2');
    return card ? card.getAttribute('data-i18n-key') : null;
  });
  expect(usesKeys).toBe('login.title');
});
```

- [ ] **Step 2: Run and confirm it fails**

Run: `npm run test:webchat-gui -- --grep "login overlay uses i18n"`
Expected: FAIL — returns `null`.

- [ ] **Step 3: Rebuild the card with elements instead of innerHTML**

In `showLoginScreen`, replace the `card.innerHTML += '<h2 …>Welcome</h2>' + '<p …>Sign in to start chatting</p>';` line with:

```js
    var titleEl = document.createElement('h2');
    titleEl.setAttribute('data-i18n-key', 'login.title');
    titleEl.textContent = uiT('login.title', 'Welcome');
    titleEl.style.cssText = 'margin:0 0 6px;font-size:1.375rem;font-weight:600;color:#1f2937;';
    card.appendChild(titleEl);
    var descEl = document.createElement('p');
    descEl.setAttribute('data-i18n-key', 'login.subtitle');
    descEl.textContent = uiT('login.subtitle', 'Sign in to start chatting');
    descEl.style.cssText = 'margin:0 0 32px;color:#6b7280;font-size:0.875rem;line-height:1.5;';
    card.appendChild(descEl);
```

Replace the button label line:

```js
      btn.textContent = /^(sign in|log in|continue)/i.test(label)
        ? label
        : uiT('login.loginWith', 'Sign in with {provider}').replace('{provider}', label);
```

Replace the empty-provider line:

```js
    if (providers.length === 0) {
      var noP = document.createElement('p');
      noP.setAttribute('data-i18n-key', 'login.noProviders');
      noP.textContent = uiT('login.noProviders', 'No OAuth providers configured.');
      noP.style.cssText = 'color:#ef4444;font-size:13px;';
      card.appendChild(noP);
    }
```

And the click handler's pending label:

```js
        btn.textContent = uiT('login.redirecting', 'Redirecting...');
```

Check `packs/messaging-webchat-gui/assets/webchat-gui/i18n/en.json` for the exact `login.loginWith` value. If it does not contain a `{provider}` placeholder, use its literal value followed by `' ' + label`, and note the discrepancy in the commit body.

- [ ] **Step 4: Run and confirm it passes**

Run: `npm run test:webchat-gui -- --grep "login overlay uses i18n"`
Expected: PASS

- [ ] **Step 5: Refresh the visual baseline**

Run: `npm run test:webchat-gui:update-snapshots`
Then inspect the diff on `tests/webchat-gui/specs/visual.spec.ts-snapshots/login-page-chromium-linux.png` and confirm the change is only the expected text, not a layout regression.

- [ ] **Step 6: Commit**

```bash
git add packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js \
  tests/webchat-gui/specs/fullscreen.spec.ts \
  tests/webchat-gui/specs/visual.spec.ts-snapshots/
git commit -m "feat(webchat-gui): translate the login overlay"
```

---

### Task 6: Declare the `greentic` provider in the shared describe surface

`oauth_bool_question` hardcodes `default: Some(json!(false))`; Greentic SSO needs a default-true variant.

**Files:**
- Modify: `components/messaging-provider-webchat/src/describe.rs` (helpers ~90, `I18N_KEYS` ~200, `oauth_questions` ~236, `I18N_PAIRS` ~350-629, `config_schema` ~800)

**Interfaces:**
- Produces: answer ids `oauth_enable_greentic` (Bool, default `true`), `oauth_greentic_issuer` (Text), `oauth_greentic_client_id` (Text). Task 7 reads them.
- Produces: config keys `oauth_greentic_issuer`, `oauth_greentic_client_id`. Task 7 adds them to the allowed-key lists.

- [ ] **Step 1: Run the existing coverage test to establish a green baseline**

Run: `cargo test -p messaging-provider-webchat --lib i18n_keys_cover_qa_specs`
Expected: PASS

- [ ] **Step 2: Add the default-true bool helper**

In `describe.rs`, immediately after `fn oauth_bool_question(...)`, add:

```rust
fn oauth_bool_question_default_on(
    id: &str,
    label_key: &str,
    help_key: &str,
    skip_if: Option<SkipExpression>,
) -> QaQuestionSpec {
    QaQuestionSpec {
        id: id.to_string(),
        label: i18n(label_key),
        help: Some(i18n(help_key)),
        help_url: None,
        error: None,
        kind: provider_common::component_v0_6::QuestionKind::Bool,
        required: false,
        default: Some(json!(true)),
        skip_if,
    }
}
```

- [ ] **Step 3: Add the questions**

In `oauth_questions()`, insert immediately after the `oauth_enabled` gate question and **before** the `// ── Google ──` block, so Greentic SSO is the first provider offered:

```rust
        // ── Greentic SSO (default) ──
        oauth_bool_question_default_on(
            "oauth_enable_greentic",
            "webchat.qa.oauth.greentic.enable",
            "webchat.qa.oauth.greentic.enable.help",
            skip_unless_oauth(),
        ),
        oauth_question(
            "oauth_greentic_issuer",
            "webchat.qa.oauth.greentic.issuer",
            "webchat.qa.oauth.greentic.issuer.help",
            false,
            skip_unless_provider("oauth_enable_greentic"),
        ),
        oauth_question(
            "oauth_greentic_client_id",
            "webchat.qa.oauth.greentic.client_id",
            "webchat.qa.oauth.greentic.client_id.help",
            false,
            skip_unless_provider("oauth_enable_greentic"),
        ),
```

- [ ] **Step 4: Add the i18n keys**

In `I18N_KEYS`, insert immediately after `"webchat.qa.oauth.enabled.help",`:

```rust
    "webchat.qa.oauth.greentic.enable",
    "webchat.qa.oauth.greentic.enable.help",
    "webchat.qa.oauth.greentic.issuer",
    "webchat.qa.oauth.greentic.issuer.help",
    "webchat.qa.oauth.greentic.client_id",
    "webchat.qa.oauth.greentic.client_id.help",
```

And, in the schema-key region of `I18N_KEYS` (near the other `webchat.schema.config.oauth_*` entries), add:

```rust
    "webchat.schema.config.oauth_greentic_issuer.title",
    "webchat.schema.config.oauth_greentic_issuer.description",
    "webchat.schema.config.oauth_greentic_client_id.title",
    "webchat.schema.config.oauth_greentic_client_id.description",
```

- [ ] **Step 5: Add the i18n pairs**

In `I18N_PAIRS`, next to `("webchat.qa.oauth.enabled", "Enable OAuth login")`, add:

```rust
    ("webchat.qa.oauth.greentic.enable", "Enable Greentic SSO"),
    (
        "webchat.qa.oauth.greentic.enable.help",
        "Sign in with the Greentic identity provider. Enabled by default.",
    ),
    ("webchat.qa.oauth.greentic.issuer", "Greentic SSO issuer URL"),
    (
        "webchat.qa.oauth.greentic.issuer.help",
        "Issuer base URL. Defaults to https://<tenant>.greentic-id.com when left empty.",
    ),
    (
        "webchat.qa.oauth.greentic.client_id",
        "Greentic SSO client ID",
    ),
    (
        "webchat.qa.oauth.greentic.client_id.help",
        "Registered public OIDC client for this tenant. Defaults to webchat-gui.",
    ),
```

And next to the `webchat.schema.config.oauth_providers.*` pairs, add:

```rust
    (
        "webchat.schema.config.oauth_greentic_issuer.title",
        "Greentic SSO issuer",
    ),
    (
        "webchat.schema.config.oauth_greentic_issuer.description",
        "Issuer base URL for the Greentic identity provider",
    ),
    (
        "webchat.schema.config.oauth_greentic_client_id.title",
        "Greentic SSO client ID",
    ),
    (
        "webchat.schema.config.oauth_greentic_client_id.description",
        "Registered public OIDC client ID for the Greentic identity provider",
    ),
```

- [ ] **Step 6: Add the config schema fields**

In `config_schema()`, immediately after the `oauth_providers` entry, add:

```rust
            (
                "oauth_greentic_issuer",
                false,
                schema_str(
                    "webchat.schema.config.oauth_greentic_issuer.title",
                    "webchat.schema.config.oauth_greentic_issuer.description",
                ),
            ),
            (
                "oauth_greentic_client_id",
                false,
                schema_str(
                    "webchat.schema.config.oauth_greentic_client_id.title",
                    "webchat.schema.config.oauth_greentic_client_id.description",
                ),
            ),
```

- [ ] **Step 7: Run the coverage and stability tests**

Run:
```bash
cargo test -p messaging-provider-webchat --lib
cargo test -p messaging-provider-webchat-gui --lib
```
Expected: PASS. `i18n_keys_cover_qa_specs` catches any key added in one list but not the other; `schema_hash_is_stable` recomputes the hash from the new schema.

- [ ] **Step 8: Commit**

```bash
git add components/messaging-provider-webchat/src/describe.rs
git commit -m "feat(webchat): declare Greentic SSO setup questions and config fields"
```

---

### Task 7: Compose and serve the `greentic` provider

**Files:**
- Modify: `components/messaging-provider-webchat-gui/src/lib.rs` (`has(...)` chain ~489, `compose_oauth_providers` ~629)
- Modify: `components/messaging-provider-webchat/src/lib.rs` (same two places)
- Modify: `components/messaging-provider-webchat-gui/src/config.rs` (allowed-key lists at ~185 and ~203)
- Modify: `components/messaging-provider-webchat/src/config.rs` (same lists)
- Modify: `components/messaging-provider-webchat/src/ops/oauth.rs` (`handle_auth_config` fallback list ~53)

**Interfaces:**
- Consumes: answer ids from Task 6.
- Produces: an `/auth/config` provider entry `{id: "greentic", label: "Greentic SSO", type: "greentic", issuer, client_id, scopes}` as the **first** array element. Task 3 consumes it.

- [ ] **Step 1: Write the failing test**

In `components/messaging-provider-webchat-gui/src/lib.rs`, inside the existing `mod tests`, add:

```rust
    #[test]
    fn greentic_sso_is_the_first_composed_provider() {
        let answers = json!({
            "oauth_enabled": true,
            "oauth_enable_greentic": true,
            "oauth_greentic_issuer": "https://acme.greentic-id.com",
            "oauth_enable_google": true,
            "oauth_google_client_id": "google-client",
        });
        let composed = compose_oauth_providers(&answers).expect("providers composed");
        let parsed: Value = serde_json::from_str(&composed).expect("valid json");
        let list = parsed.as_array().expect("array");
        assert_eq!(list[0]["id"], json!("greentic"));
        assert_eq!(list[0]["type"], json!("greentic"));
        assert_eq!(list[0]["client_id"], json!("webchat-gui"));
        assert_eq!(list[0]["issuer"], json!("https://acme.greentic-id.com"));
        assert_eq!(list[1]["id"], json!("google"));
    }
```

- [ ] **Step 2: Run and confirm it fails**

Run: `cargo test -p messaging-provider-webchat-gui --lib greentic_sso_is_the_first`
Expected: FAIL — index 0 is `google`.

- [ ] **Step 3: Compose the provider first**

In `compose_oauth_providers` in `components/messaging-provider-webchat-gui/src/lib.rs`, insert immediately after `let mut providers: Vec<Value> = Vec::new();` and **before** the Google block:

```rust
    if is_truthy(answers, "oauth_enable_greentic") {
        let mut p = json!({
            "id": "greentic",
            "label": "Greentic SSO",
            "type": "greentic",
            "client_id": optional_string_from(answers, "oauth_greentic_client_id")
                .unwrap_or_else(|| "webchat-gui".to_string()),
            "scopes": "openid profile email greentic.webchat"
        });
        if let Some(issuer) = optional_string_from(answers, "oauth_greentic_issuer") {
            p["issuer"] = Value::String(issuer);
        }
        providers.push(p);
    }
```

Apply the identical block to `compose_oauth_providers` in `components/messaging-provider-webchat/src/lib.rs`.

- [ ] **Step 4: Add the toggle to the apply gate**

In both `lib.rs` files, extend the `has(...)` chain:

```rust
        if has("oauth_enable_greentic")
            || has("oauth_enable_google")
            || has("oauth_enable_microsoft")
            || has("oauth_enable_github")
            || has("oauth_enable_custom")
            || has("oauth_providers")
        {
            merged.oauth_providers = compose_oauth_providers(&answers);
        }
```

- [ ] **Step 5: Add the config keys**

In `components/messaging-provider-webchat-gui/src/config.rs`, add `"oauth_greentic_issuer"` and `"oauth_greentic_client_id"` to **both** key arrays (the plain list and the `_b64` list). Do the same in `components/messaging-provider-webchat/src/config.rs`.

`decode_injected_config_field` leaves unlisted keys as `String`, which is correct for both new fields — no change needed there.

- [ ] **Step 6: Serve the provider in the fallback path**

In `components/messaging-provider-webchat/src/ops/oauth.rs`, inside `handle_auth_config`'s `else` branch (the per-field fallback used when `oauth_providers` is absent), insert immediately after `let mut list = Vec::new();`:

```rust
            if read_secret("oauth_enable_greentic").as_deref() == Some("true") {
                let client_id = read_secret("oauth_greentic_client_id")
                    .unwrap_or_else(|| "webchat-gui".to_string());
                let mut entry = json!({
                    "id": "greentic", "label": "Greentic SSO", "type": "greentic",
                    "client_id": client_id,
                    "scopes": "openid profile email greentic.webchat"
                });
                if let Some(issuer) = read_secret("oauth_greentic_issuer") {
                    entry["issuer"] = Value::String(issuer);
                }
                list.push(entry);
            }
```

- [ ] **Step 7: Run and confirm it passes**

Run:
```bash
cargo test -p messaging-provider-webchat-gui --lib
cargo test -p messaging-provider-webchat --lib
```
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add components/messaging-provider-webchat-gui/src/lib.rs \
  components/messaging-provider-webchat-gui/src/config.rs \
  components/messaging-provider-webchat/src/lib.rs \
  components/messaging-provider-webchat/src/config.rs \
  components/messaging-provider-webchat/src/ops/oauth.rs
git commit -m "feat(webchat): compose Greentic SSO as the first auth provider"
```

---

### Task 8: Pack metadata, JSON schemas and fixtures

**Files:**
- Modify: `packs/messaging-webchat-gui/pack.yaml` (assets list)
- Modify: `packs/messaging-webchat-gui/pack.manifest.json` (assets list)
- Modify: `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/config.schema.json`
- Modify: `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/public.config.schema.json`
- Modify: `packs/messaging-webchat-gui/assets/webchat-gui/config/tenants/greentic.json`
- Modify: `tools/import_webchat_gui_assets.sh` (the `default.json` heredoc)
- Modify: `crates/provider-common/tests/pack_metadata.rs`
- Regenerate: `tests/fixtures/registry/webchat/*.cbor`
- Modify: `packs/messaging-webchat-gui/fixtures/setup.input.json`, `setup.expected.plan.json`

**Interfaces:**
- Consumes: config keys from Tasks 6 and 7; the asset filenames from Tasks 1 and 2.

- [ ] **Step 1: Write the failing pack test**

In `crates/provider-common/tests/pack_metadata.rs`, inside `webchat_gui_pack_contains_runtime_bootstrap_and_bundled_assets`, add `"greentic-sso.js"` and `"sso-callback.html"` to the list of files asserted to exist, and add:

```rust
    assert!(
        pack_yaml.contains("assets/webchat-gui/greentic-sso.js"),
        "pack.yaml must declare the bundled SSO SDK asset"
    );
```

- [ ] **Step 2: Run and confirm it fails**

Run: `cargo test -p greentic-messaging-provider-common --test pack_metadata webchat_gui_pack_contains`
Expected: FAIL

- [ ] **Step 3: Declare the assets**

In `packs/messaging-webchat-gui/pack.yaml`, add to the `assets:` list:

```yaml
- path: assets/webchat-gui/greentic-sso.js
- path: assets/webchat-gui/sso-callback.html
```

Add the same two entries to the `"assets"` array in `packs/messaging-webchat-gui/pack.manifest.json` as `{"path": "assets/webchat-gui/greentic-sso.js"}` and `{"path": "assets/webchat-gui/sso-callback.html"}`.

- [ ] **Step 4: Add the new fields to both JSON schemas**

Both files use `"additionalProperties": false`, so a persisted field absent from them can be rejected by any consumer validating against them. Add to the `properties` object of **both** `config.schema.json` and `public.config.schema.json`:

```json
    "oauth_enabled": {
      "type": "boolean",
      "description": "Require OAuth authentication before the chat loads."
    },
    "oauth_providers": {
      "type": "string",
      "description": "JSON array of configured OAuth providers."
    },
    "oauth_greentic_issuer": {
      "type": "string",
      "format": "uri",
      "description": "Issuer base URL for the Greentic identity provider."
    },
    "oauth_greentic_client_id": {
      "type": "string",
      "description": "Registered public OIDC client ID for the Greentic identity provider."
    }
```

`oauth_enabled` and `oauth_providers` are added here because the runtime already persists them and the schemas already reject them — adding the two new fields without them would leave the same hole open for this feature.

- [ ] **Step 5: Put Greentic SSO first in the tenant configs**

In `packs/messaging-webchat-gui/assets/webchat-gui/config/tenants/greentic.json`, replace the `auth.providers` array with:

```json
  "auth": {
    "providers": [
      {
        "id": "greentic",
        "label": "Greentic SSO",
        "type": "greentic",
        "enabled": true,
        "clientId": "webchat-gui",
        "scope": "openid profile email greentic.webchat"
      },
      {
        "id": "greentic-demo",
        "label": "Greentic Demo",
        "type": "dummy",
        "enabled": true
      },
      {
        "id": "greentic-microsoft",
        "label": "Microsoft",
        "type": "oidc",
        "enabled": true,
        "authorizationUrl": "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        "clientId": "greentic-webchat-demo",
        "redirectUri": "https://example.com/auth/callback/greentic-microsoft",
        "scope": "openid profile email",
        "responseType": "code",
        "prompt": "login"
      }
    ]
  },
```

In `tools/import_webchat_gui_assets.sh`, the `default.json` heredoc fully overwrites that file on every import. Change its `"auth": {"providers": []}` line to:

```python
    "auth": {
        "providers": [
            {
                "id": "greentic",
                "label": "Greentic SSO",
                "type": "greentic",
                "enabled": True,
                "clientId": "webchat-gui",
                "scope": "openid profile email greentic.webchat",
            }
        ]
    },
```

Then apply the same `auth.providers` array to the committed `packs/messaging-webchat-gui/assets/webchat-gui/config/tenants/default.json`.

`pack_metadata.rs:1577` asserts `default.json` carries an **enabled `type: "dummy"` provider**. Adding `greentic` first does not remove the dummy entry from `greentic.json`, but `default.json` currently has an empty list — check whether that test reads `default.json` or `greentic.json` and adjust the assertion to look for the `greentic` entry if it reads `default.json`.

- [ ] **Step 6: Update the pack setup fixtures**

Add `"oauth_enabled": true` and `"oauth_enable_greentic": true` to `packs/messaging-webchat-gui/fixtures/setup.input.json`, then run:

```bash
python3 tools/validate_pack_fixtures.py
```
Read the reported diff and update `setup.expected.plan.json` to match the new plan output.

- [ ] **Step 7: Regenerate the registry fixtures**

Run: `./tools/regenerate_registry_fixtures.sh`
Expected: `tests/fixtures/registry/webchat/describe.cbor` and `qa_setup.cbor` change.

- [ ] **Step 8: Run the full check**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add packs/messaging-webchat-gui/pack.yaml packs/messaging-webchat-gui/pack.manifest.json \
  packs/messaging-webchat-gui/schemas/ packs/messaging-webchat-gui/assets/webchat-gui/config/tenants/ \
  packs/messaging-webchat-gui/fixtures/ tools/import_webchat_gui_assets.sh \
  crates/provider-common/tests/pack_metadata.rs tests/fixtures/registry/webchat/
git commit -m "feat(webchat-gui): register SSO assets and default the greentic provider"
```

---

### Task 9: Close out PR 1

**Files:**
- Modify: `scripts/test_webchat_gui.sh` (the `--login` provider injection block, ~282-293)

- [ ] **Step 1: Add a greentic entry to the manual harness**

In `scripts/test_webchat_gui.sh`, in the inline Python block where `--login` sets `auth.providers`, replace the injected list with:

```python
        config["auth"] = {"providers": [
            {"id": "greentic", "label": "Greentic SSO", "type": "greentic",
             "enabled": True, "clientId": "webchat-gui",
             "scope": "openid profile email greentic.webchat"},
            {"id": "guest", "label": "Continue as Guest", "type": "dummy", "enabled": True},
        ]}
```

- [ ] **Step 2: Run the full local check**

Run: `./ci/local_check.sh`
Expected: every step passes. If `07_sync_packs.sh` downbumps pack versions, revert only the version fields — see the gotcha in `CLAUDE.md`.

- [ ] **Step 3: Run the full Playwright suite**

Run: `npm run test:webchat-gui`
Expected: PASS

- [ ] **Step 4: Commit and open PR 1**

```bash
git add scripts/test_webchat_gui.sh
git commit -m "test(webchat-gui): offer Greentic SSO in the manual login harness"
```

---

# PR 2 — verified chat-token mint (layer D)

---

### Task 10: ES256 access-token verifier

The verifier is a pure function over `(token, jwks_json, expectations, now)` so it can be unit-tested natively, with no WASM host and no network.

**Files:**
- Create: `components/messaging-provider-webchat/src/directline/oidc.rs`
- Create: `crates/webchat-directline-core/src/directline/oidc.rs` (an `include!` shim, mirroring the existing `http.rs` / `jwt.rs` / `state.rs` shims)
- Modify: `components/messaging-provider-webchat/src/directline/mod.rs`
- Modify: `crates/webchat-directline-core/Cargo.toml`, `components/messaging-provider-webchat/Cargo.toml`, `components/messaging-provider-webchat-gui/Cargo.toml`, root `Cargo.toml`

**Interfaces:**
- Produces:
  ```rust
  pub struct VerifiedIdentity { pub sub: String }
  pub enum OidcError { InvalidFormat, UnsupportedAlg, UnknownKey, InvalidSignature, Expired, NotYetValid, IssuerMismatch, AudienceMismatch, MissingScope }
  pub fn verify_access_token(
      token: &str, jwks_json: &str, expected_iss: &str,
      expected_aud: &str, required_scope: &str, now: i64,
  ) -> Result<VerifiedIdentity, OidcError>;
  ```
  Task 11 consumes this.

- [ ] **Step 1: Resolve the dependency before writing any code**

The workspace pins `sha2 = "0.11"` and `hmac = "0.13"`, which are the next-generation RustCrypto releases. `p256` may depend on an older `sha2`. Two `sha2` majors in the tree compile fine, so accept the duplicate rather than forcing a match.

Add to the root `Cargo.toml` `[workspace.dependencies]`:

```toml
p256 = { version = "0.13", default-features = false, features = ["ecdsa", "std"] }
```

Add `p256.workspace = true` to the `[dependencies]` of `crates/webchat-directline-core/Cargo.toml`, `components/messaging-provider-webchat/Cargo.toml` and `components/messaging-provider-webchat-gui/Cargo.toml`.

Run:
```bash
cargo check -p webchat-directline-core
cargo build -p messaging-provider-webchat --target wasm32-wasip2
```
Expected: both succeed. If the wasm build fails on a `getrandom` backend, drop `std` from the feature list and retry; ECDSA *verification* needs no RNG. If it still fails, stop and report — the dependency choice is the blocker, not the code.

- [ ] **Step 2: Write the failing tests**

Create `components/messaging-provider-webchat/src/directline/oidc.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const ISS: &str = "https://acme.greentic-id.com";
    const AUD: &str = "webchat-gui";
    const SCOPE: &str = "greentic.webchat";

    fn fixture() -> (String, String) {
        // Returns (token, jwks_json) for a token signed with a freshly
        // generated P-256 key, claims: iss=ISS, aud=AUD, sub="acme:users:1",
        // scope="openid profile email greentic.webchat", exp=2_000_000_000.
        crate::directline::oidc_test_support::signed_fixture(
            ISS, AUD, "acme:users:1",
            "openid profile email greentic.webchat", 2_000_000_000,
        )
    }

    #[test]
    fn accepts_a_valid_token() {
        let (token, jwks) = fixture();
        let id = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 1_900_000_000)
            .expect("token accepted");
        assert_eq!(id.sub, "acme:users:1");
    }

    #[test]
    fn rejects_an_expired_token() {
        let (token, jwks) = fixture();
        let err = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 2_000_000_001).unwrap_err();
        assert!(matches!(err, OidcError::Expired));
    }

    #[test]
    fn rejects_a_foreign_issuer() {
        let (token, jwks) = fixture();
        let err = verify_access_token(&token, &jwks, "https://evil.example", AUD, SCOPE, 1_900_000_000)
            .unwrap_err();
        assert!(matches!(err, OidcError::IssuerMismatch));
    }

    #[test]
    fn rejects_a_foreign_audience() {
        let (token, jwks) = fixture();
        let err = verify_access_token(&token, &jwks, ISS, "someone-else", SCOPE, 1_900_000_000)
            .unwrap_err();
        assert!(matches!(err, OidcError::AudienceMismatch));
    }

    #[test]
    fn rejects_a_token_without_the_required_scope() {
        let (token, jwks) = crate::directline::oidc_test_support::signed_fixture(
            ISS, AUD, "acme:users:1", "openid profile email", 2_000_000_000,
        );
        let err = verify_access_token(&token, &jwks, ISS, AUD, SCOPE, 1_900_000_000).unwrap_err();
        assert!(matches!(err, OidcError::MissingScope));
    }

    #[test]
    fn rejects_a_tampered_signature() {
        let (token, jwks) = fixture();
        let mut parts: Vec<&str> = token.split('.').collect();
        let tampered_sig = "AAAA".to_string() + &parts[2][4..];
        parts[2] = &tampered_sig;
        let bad = parts.join(".");
        let err = verify_access_token(&bad, &jwks, ISS, AUD, SCOPE, 1_900_000_000).unwrap_err();
        assert!(matches!(err, OidcError::InvalidSignature));
    }

    #[test]
    fn rejects_a_key_the_jwks_does_not_carry() {
        let (token, _) = fixture();
        let (_, other_jwks) = crate::directline::oidc_test_support::signed_fixture(
            ISS, AUD, "acme:users:2", "greentic.webchat", 2_000_000_000,
        );
        let err = verify_access_token(&token, &other_jwks, ISS, AUD, SCOPE, 1_900_000_000)
            .unwrap_err();
        assert!(matches!(err, OidcError::UnknownKey));
    }
}
```

Create the signing helper `components/messaging-provider-webchat/src/directline/oidc_test_support.rs`:

```rust
#![cfg(test)]

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use p256::elliptic_curve::sec1::ToEncodedPoint;

/// Deterministic-per-call P-256 keypair; each call mints a distinct `kid`.
pub fn signed_fixture(
    iss: &str,
    aud: &str,
    sub: &str,
    scope: &str,
    exp: i64,
) -> (String, String) {
    let signing_key = SigningKey::random(&mut rand_core::OsRng);
    let point = signing_key.verifying_key().to_encoded_point(false);
    let kid = URL_SAFE_NO_PAD.encode(&point.as_bytes()[1..9]);

    let header = serde_json::json!({"alg": "ES256", "typ": "JWT", "kid": kid});
    let claims = serde_json::json!({
        "iss": iss, "aud": aud, "sub": sub, "scope": scope,
        "exp": exp, "iat": exp - 3600,
    });
    let header_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header json"));
    let claims_enc = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("claims json"));
    let signing_input = format!("{header_enc}.{claims_enc}");
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let token = format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()));

    let jwks = serde_json::json!({"keys": [{
        "kty": "EC", "crv": "P-256", "alg": "ES256", "kid": kid,
        "x": URL_SAFE_NO_PAD.encode(&point.as_bytes()[1..33]),
        "y": URL_SAFE_NO_PAD.encode(&point.as_bytes()[33..65]),
    }]});

    (token, jwks.to_string())
}
```

Add `rand_core = "0.6"` as a `[dev-dependencies]` entry to `crates/webchat-directline-core/Cargo.toml`, and declare both modules in `components/messaging-provider-webchat/src/directline/mod.rs`:

```rust
pub mod oidc;
#[cfg(test)]
mod oidc_test_support;
```

- [ ] **Step 3: Run and confirm they fail**

Run: `cargo test -p webchat-directline-core oidc`
Expected: FAIL — `verify_access_token` not defined.

- [ ] **Step 4: Implement the verifier**

Prepend to `components/messaging-provider-webchat/src/directline/oidc.rs`, above the test module:

```rust
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::EncodedPoint;
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::Deserialize;

pub struct VerifiedIdentity {
    pub sub: String,
}

#[derive(Debug)]
pub enum OidcError {
    InvalidFormat,
    UnsupportedAlg,
    UnknownKey,
    InvalidSignature,
    Expired,
    NotYetValid,
    IssuerMismatch,
    AudienceMismatch,
    MissingScope,
}

#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    #[serde(default)]
    kid: Option<String>,
}

#[derive(Deserialize)]
struct AccessClaims {
    iss: String,
    sub: String,
    aud: String,
    #[serde(default)]
    scope: String,
    exp: i64,
    #[serde(default)]
    nbf: Option<i64>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    crv: Option<String>,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    x: Option<String>,
    #[serde(default)]
    y: Option<String>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

fn decode_part<T: for<'de> Deserialize<'de>>(part: &str) -> Result<T, OidcError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part)
        .map_err(|_| OidcError::InvalidFormat)?;
    serde_json::from_slice(&bytes).map_err(|_| OidcError::InvalidFormat)
}

fn verifying_key(jwk: &Jwk) -> Result<VerifyingKey, OidcError> {
    if jwk.kty != "EC" || jwk.crv.as_deref() != Some("P-256") {
        return Err(OidcError::UnsupportedAlg);
    }
    let x = jwk.x.as_deref().ok_or(OidcError::UnknownKey)?;
    let y = jwk.y.as_deref().ok_or(OidcError::UnknownKey)?;
    let x_bytes = URL_SAFE_NO_PAD.decode(x).map_err(|_| OidcError::UnknownKey)?;
    let y_bytes = URL_SAFE_NO_PAD.decode(y).map_err(|_| OidcError::UnknownKey)?;
    if x_bytes.len() != 32 || y_bytes.len() != 32 {
        return Err(OidcError::UnknownKey);
    }
    let point = EncodedPoint::from_affine_coordinates(
        x_bytes.as_slice().into(),
        y_bytes.as_slice().into(),
        false,
    );
    VerifyingKey::from_encoded_point(&point).map_err(|_| OidcError::UnknownKey)
}

pub fn verify_access_token(
    token: &str,
    jwks_json: &str,
    expected_iss: &str,
    expected_aud: &str,
    required_scope: &str,
    now: i64,
) -> Result<VerifiedIdentity, OidcError> {
    let mut parts = token.split('.');
    let header_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    let claims_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    let sig_enc = parts.next().ok_or(OidcError::InvalidFormat)?;
    if parts.next().is_some() {
        return Err(OidcError::InvalidFormat);
    }

    let header: JwtHeader = decode_part(header_enc)?;
    if header.alg != "ES256" {
        return Err(OidcError::UnsupportedAlg);
    }

    let jwks: Jwks = serde_json::from_str(jwks_json).map_err(|_| OidcError::UnknownKey)?;
    let jwk = jwks
        .keys
        .iter()
        .find(|k| match (&header.kid, &k.kid) {
            (Some(want), Some(have)) => want == have,
            (None, _) => true,
            _ => false,
        })
        .ok_or(OidcError::UnknownKey)?;
    let key = verifying_key(jwk)?;

    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_enc)
        .map_err(|_| OidcError::InvalidFormat)?;
    let signature =
        Signature::from_slice(&sig_bytes).map_err(|_| OidcError::InvalidSignature)?;
    let signing_input = format!("{header_enc}.{claims_enc}");
    key.verify(signing_input.as_bytes(), &signature)
        .map_err(|_| OidcError::InvalidSignature)?;

    let claims: AccessClaims = decode_part(claims_enc)?;
    if claims.iss != expected_iss {
        return Err(OidcError::IssuerMismatch);
    }
    if claims.aud != expected_aud {
        return Err(OidcError::AudienceMismatch);
    }
    if now >= claims.exp {
        return Err(OidcError::Expired);
    }
    if let Some(nbf) = claims.nbf
        && now < nbf
    {
        return Err(OidcError::NotYetValid);
    }
    if !required_scope.is_empty()
        && !claims
            .scope
            .split_whitespace()
            .any(|s| s == required_scope)
    {
        return Err(OidcError::MissingScope);
    }

    Ok(VerifiedIdentity { sub: claims.sub })
}
```

Create the shim `crates/webchat-directline-core/src/directline/oidc.rs` mirroring the existing shims:

```rust
include!("../../../../components/messaging-provider-webchat/src/directline/oidc.rs");
```

Match the exact relative depth used by the existing `http.rs` shim in that directory.

- [ ] **Step 5: Run and confirm all seven pass**

Run: `cargo test -p webchat-directline-core oidc`
Expected: 7 passed.

- [ ] **Step 6: Check the file stays under 500 lines**

Run: `wc -l components/messaging-provider-webchat/src/directline/oidc.rs`
Expected: under 500. If not, move the test module into `oidc_tests.rs` and include it with `#[cfg(test)] #[path = "oidc_tests.rs"] mod tests;`, matching the `sso_state_tests.rs` split pattern used elsewhere in the Greentic codebase.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/webchat-directline-core/ \
  components/messaging-provider-webchat/src/directline/ \
  components/messaging-provider-webchat/Cargo.toml \
  components/messaging-provider-webchat-gui/Cargo.toml
git commit -m "feat(webchat): add an ES256 OIDC access-token verifier"
```

---

### Task 11: Wire verification into the token mint

`handle_directline_request` has 20+ call sites, almost all in tests. A wrapper keeps those untouched.

**Files:**
- Modify: `components/messaging-provider-webchat/src/directline/http.rs` (`handle_directline_request` ~34, `handle_tokens` ~91)
- Modify: `components/messaging-provider-webchat/src/directline/store.rs` (new trait)
- Modify: `components/messaging-provider-webchat/src/directline/host.rs` and `components/messaging-provider-webchat-gui/src/directline/host.rs` (host impl)
- Modify: `components/messaging-provider-webchat/src/ops/ingest.rs:94,158`
- Modify: `components/messaging-provider-webchat/src/directline/jwt.rs` (`TokenClaims`, `issue_token`)

**Interfaces:**
- Consumes: `verify_access_token`, `VerifiedIdentity`, `OidcError` from Task 10.
- Produces:
  ```rust
  pub trait JwksFetcher { fn fetch(&self, jwks_url: &str) -> Result<String, String>; }
  pub struct NoJwksFetcher;
  pub fn handle_directline_request_with_jwks<S, SE, J>(
      request: &HttpInV1, state_store: &mut S, secrets: &SE, jwks: &J,
  ) -> HttpOutV1 where S: StateStore, SE: SecretStore, J: JwksFetcher;
  ```
- Produces: `TokenClaims.verified: bool`, and `issue_token(secret, ctx, sub, conv, verified)`.

- [ ] **Step 1: Write the failing test**

In `components/messaging-provider-webchat/src/directline/http.rs`'s test module, add:

```rust
    struct StaticJwks(String);
    impl JwksFetcher for StaticJwks {
        fn fetch(&self, _url: &str) -> Result<String, String> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn bearer_token_mints_an_identity_bound_direct_line_token() {
        let (access_token, jwks) = crate::directline::oidc_test_support::signed_fixture(
            "https://acme.greentic-id.com", "webchat-gui", "acme:users:7",
            "openid greentic.webchat", 4_000_000_000,
        );
        let mut state = MemoryStateStore::default();
        let secrets = MapSecretStore::with_signing_key();
        let mut request = token_request_with_config(json!({
            "oidc_issuer": "https://acme.greentic-id.com",
            "oidc_audience": "webchat-gui",
            "oidc_required_scope": "greentic.webchat",
        }));
        request.headers.push(Header {
            name: "Authorization".into(),
            value: format!("Bearer {access_token}"),
        });

        let response = handle_directline_request_with_jwks(
            &request, &mut state, &secrets, &StaticJwks(jwks),
        );
        assert_eq!(response.status, 200);

        let body: Value = serde_json::from_slice(&decode_body(&response)).expect("json body");
        let token = body["token"].as_str().expect("token string");
        let claims = verify_token(b"test-signing-key", token).expect("direct line token verifies");
        assert_eq!(claims.sub, "acme:users:7");
        assert!(claims.verified);
    }

    #[test]
    fn an_invalid_bearer_is_rejected_with_401() {
        let (_, jwks) = crate::directline::oidc_test_support::signed_fixture(
            "https://acme.greentic-id.com", "webchat-gui", "acme:users:7",
            "greentic.webchat", 4_000_000_000,
        );
        let mut state = MemoryStateStore::default();
        let secrets = MapSecretStore::with_signing_key();
        let mut request = token_request_with_config(json!({
            "oidc_issuer": "https://acme.greentic-id.com",
            "oidc_audience": "webchat-gui",
            "oidc_required_scope": "greentic.webchat",
        }));
        request.headers.push(Header {
            name: "Authorization".into(),
            value: "Bearer not-a-jwt".into(),
        });

        let response = handle_directline_request_with_jwks(
            &request, &mut state, &secrets, &StaticJwks(jwks),
        );
        assert_eq!(response.status, 401);
    }

    #[test]
    fn no_bearer_still_mints_an_anonymous_token() {
        let mut state = MemoryStateStore::default();
        let secrets = MapSecretStore::with_signing_key();
        let request = token_request_with_config(json!({
            "oidc_issuer": "https://acme.greentic-id.com",
        }));
        let response = handle_directline_request_with_jwks(
            &request, &mut state, &secrets, &NoJwksFetcher,
        );
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_slice(&decode_body(&response)).expect("json body");
        let claims = verify_token(b"test-signing-key", body["token"].as_str().expect("token"))
            .expect("verifies");
        assert!(!claims.verified);
    }
```

Reuse the existing test helpers in that module for `MemoryStateStore`, the secret store and body decoding. If a `token_request_with_config` helper does not exist, add one modelled on the existing token-request builder, taking a `serde_json::Value` and setting it as `request.config`.

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test -p webchat-directline-core bearer_token_mints`
Expected: FAIL — `JwksFetcher` not defined.

- [ ] **Step 3: Add the trait and the null implementation**

In `components/messaging-provider-webchat/src/directline/store.rs`, next to `SecretStore`:

```rust
/// Driver for fetching an OIDC issuer's JWKS document.
pub trait JwksFetcher {
    fn fetch(&self, jwks_url: &str) -> Result<String, String>;
}

/// Fetcher for contexts with no outbound HTTP. Verification fails closed.
pub struct NoJwksFetcher;

impl JwksFetcher for NoJwksFetcher {
    fn fetch(&self, _jwks_url: &str) -> Result<String, String> {
        Err("jwks fetching unavailable".to_string())
    }
}
```

- [ ] **Step 4: Add the host implementation**

In `components/messaging-provider-webchat/src/directline/host.rs` and the identical file under `messaging-provider-webchat-gui`:

```rust
pub struct HostJwksFetcher;

impl JwksFetcher for HostJwksFetcher {
    fn fetch(&self, jwks_url: &str) -> Result<String, String> {
        let request = client::Request {
            method: "GET".into(),
            url: jwks_url.to_string(),
            headers: vec![("Accept".into(), "application/json".into())],
            body: None,
        };
        let response = client::send(&request, None, None)
            .map_err(|err| format!("jwks request failed: {}", err.message))?;
        if response.status != 200 {
            return Err(format!("jwks endpoint returned {}", response.status));
        }
        String::from_utf8(response.body.unwrap_or_default())
            .map_err(|_| "jwks response not utf-8".to_string())
    }
}
```

Import `crate::bindings::greentic::http::http_client as client` in both files, matching the import already used in `ops/oauth.rs`.

- [ ] **Step 5: Thread the fetcher through the router**

In `http.rs`, rename the existing `handle_directline_request` body into a new generic function and keep the old name as a wrapper:

```rust
pub fn handle_directline_request<S, SE>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
{
    handle_directline_request_with_jwks(request, state_store, secrets, &NoJwksFetcher)
}

pub fn handle_directline_request_with_jwks<S, SE, J>(
    request: &HttpInV1,
    state_store: &mut S,
    secrets: &SE,
    jwks: &J,
) -> HttpOutV1
where
    S: StateStore,
    SE: SecretStore,
    J: JwksFetcher,
{
    // …the existing body, with the tokens/generate arm calling
    // handle_tokens(request, state_store, secrets, jwks)
}
```

Every other match arm keeps its current three-argument call.

- [ ] **Step 6: Verify the bearer in `handle_tokens`**

Change the signature to accept `jwks: &J` and insert, immediately after `let subject = determine_rate_limit_subject(request, &body);`:

```rust
    let issuer = config_str(request, "oidc_issuer");
    let bearer = extract_bearer(&request.headers);
    let (token_subject, verified) = match (issuer.as_deref(), bearer.as_deref()) {
        (Some(issuer), Some(bearer)) => {
            let audience =
                config_str(request, "oidc_audience").unwrap_or_else(|| "webchat-gui".to_string());
            let required_scope = config_str(request, "oidc_required_scope")
                .unwrap_or_else(|| "greentic.webchat".to_string());
            let jwks_doc = match jwks.fetch(&format!("{}/jwks.json", issuer.trim_end_matches('/'))) {
                Ok(doc) => doc,
                Err(err) => {
                    return respond_error(401, "unauthorized", format!("jwks unavailable: {err}"));
                }
            };
            match verify_access_token(bearer, &jwks_doc, issuer, &audience, &required_scope, now) {
                Ok(identity) => (identity.sub, true),
                Err(err) => {
                    return respond_error(
                        401,
                        "unauthorized",
                        format!("access token rejected: {err:?}"),
                    );
                }
            }
        }
        _ => (subject.token_subject().to_string(), false),
    };
```

Then change the mint call:

```rust
    match issue_token(&signing_key, ctx.clone(), &token_subject, None, verified) {
```

Add a `config_str` helper next to `load_signing_key` if one does not already exist:

```rust
fn config_str(request: &HttpInV1, key: &str) -> Option<String> {
    request
        .config
        .as_ref()
        .and_then(|cfg| cfg.get(key))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
```

Rate limiting stays where it is, before verification, so an unauthenticated flood is still bucketed.

- [ ] **Step 7: Cache the JWKS document**

A JWKS fetch on every mint adds an outbound round trip to each page load. Cache
it in the state store that `handle_tokens` already holds.

In `http.rs`, add next to `config_str`:

```rust
const JWKS_CACHE_TTL_SECONDS: i64 = 15 * 60;

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedJwks {
    document: String,
    fetched_at: i64,
}

fn load_jwks<S, J>(
    state_store: &mut S,
    jwks: &J,
    jwks_url: &str,
    now: i64,
) -> Result<String, String>
where
    S: StateStore,
    J: JwksFetcher,
{
    let cache_key = format!("webchat:jwks:{jwks_url}");
    if let Ok(Some(bytes)) = state_store.read(&cache_key)
        && let Ok(cached) = serde_json::from_slice::<CachedJwks>(&bytes)
        && now - cached.fetched_at < JWKS_CACHE_TTL_SECONDS
    {
        return Ok(cached.document);
    }
    let document = jwks.fetch(jwks_url)?;
    let entry = CachedJwks {
        document: document.clone(),
        fetched_at: now,
    };
    if let Ok(bytes) = serde_json::to_vec(&entry) {
        let _ = state_store.write(&cache_key, &bytes);
    }
    Ok(document)
}
```

Then in the verification block from Step 6, replace the direct
`jwks.fetch(...)` call with:

```rust
            let jwks_url = format!("{}/jwks.json", issuer.trim_end_matches('/'));
            let jwks_doc = match load_jwks(state_store, jwks, &jwks_url, now) {
```

A cache write failure is deliberately ignored: it costs a refetch next time, and
failing the mint over it would turn a degraded state store into a login outage.

Add a test asserting the second mint does not refetch:

```rust
    struct CountingJwks {
        document: String,
        calls: std::cell::Cell<usize>,
    }
    impl JwksFetcher for CountingJwks {
        fn fetch(&self, _url: &str) -> Result<String, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.document.clone())
        }
    }

    #[test]
    fn jwks_is_fetched_once_across_two_mints() {
        let (access_token, jwks_doc) = crate::directline::oidc_test_support::signed_fixture(
            "https://acme.greentic-id.com", "webchat-gui", "acme:users:7",
            "greentic.webchat", 4_000_000_000,
        );
        let fetcher = CountingJwks { document: jwks_doc, calls: std::cell::Cell::new(0) };
        let mut state = MemoryStateStore::default();
        let secrets = MapSecretStore::with_signing_key();
        let mut request = token_request_with_config(json!({
            "oidc_issuer": "https://acme.greentic-id.com",
            "oidc_audience": "webchat-gui",
            "oidc_required_scope": "greentic.webchat",
            "rate_limit_requests": 100,
        }));
        request.headers.push(Header {
            name: "Authorization".into(),
            value: format!("Bearer {access_token}"),
        });

        for _ in 0..2 {
            let response =
                handle_directline_request_with_jwks(&request, &mut state, &secrets, &fetcher);
            assert_eq!(response.status, 200);
        }
        assert_eq!(fetcher.calls.get(), 1);
    }
```

- [ ] **Step 8: Carry `verified` in the DirectLine claims**

In `jwt.rs`, add to `TokenClaims`:

```rust
    #[serde(default)]
    pub verified: bool,
```

Add a `verified: bool` parameter to `issue_token` and set it in the constructed `TokenClaims`. Update the other `issue_token` call sites — `handle_conversations` and `handle_refresh_token` — to forward the calling token's `claims.verified` rather than hardcoding `false`, so a verified session keeps its identity across conversation creation and refresh.

- [ ] **Step 9: Switch ingest to the jwks-aware entry point**

In `components/messaging-provider-webchat/src/ops/ingest.rs`, change the import and both call sites at lines 94 and 158 to use `handle_directline_request_with_jwks(…, &HostJwksFetcher)`.

- [ ] **Step 10: Run and confirm all four pass**

Run: `cargo test -p webchat-directline-core`
Expected: PASS, including the pre-existing DirectLine tests, which still call the three-argument wrapper.

- [ ] **Step 11: Commit**

```bash
git add components/messaging-provider-webchat/src/directline/ \
  components/messaging-provider-webchat/src/ops/ingest.rs \
  components/messaging-provider-webchat-gui/src/directline/
git commit -m "feat(webchat): mint identity-bound Direct Line tokens from verified bearers"
```

---

### Task 12: Declare the OIDC verification config

**Files:**
- Modify: `components/messaging-provider-webchat/src/describe.rs`
- Modify: `components/messaging-provider-webchat-gui/src/config.rs`, `components/messaging-provider-webchat/src/config.rs`
- Modify: `components/messaging-provider-webchat-gui/src/lib.rs`, `components/messaging-provider-webchat/src/lib.rs`
- Modify: both JSON schemas under `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/`
- Regenerate: `tests/fixtures/registry/webchat/*.cbor`

**Interfaces:**
- Produces: config keys `oidc_issuer`, `oidc_audience`, `oidc_required_scope`, read by `handle_tokens` (Task 11).

- [ ] **Step 1: Add the struct fields**

In `components/messaging-provider-webchat-gui/src/config.rs`, add to `ProviderConfig` and `ProviderConfigOut`:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_issuer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_audience: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_required_scope: Option<String>,
```

Add the three key names to **both** allowed-key arrays in that file. Mirror all of it in `components/messaging-provider-webchat/src/config.rs`.

- [ ] **Step 2: Add the schema entries and i18n**

In `describe.rs` `config_schema()`, after the `oauth_greentic_client_id` entry from Task 6:

```rust
            (
                "oidc_issuer",
                false,
                schema_str(
                    "webchat.schema.config.oidc_issuer.title",
                    "webchat.schema.config.oidc_issuer.description",
                ),
            ),
            (
                "oidc_audience",
                false,
                schema_str(
                    "webchat.schema.config.oidc_audience.title",
                    "webchat.schema.config.oidc_audience.description",
                ),
            ),
            (
                "oidc_required_scope",
                false,
                schema_str(
                    "webchat.schema.config.oidc_required_scope.title",
                    "webchat.schema.config.oidc_required_scope.description",
                ),
            ),
```

Add the six matching keys to `I18N_KEYS` and the six pairs to `I18N_PAIRS`:

```rust
    (
        "webchat.schema.config.oidc_issuer.title",
        "OIDC issuer for chat-token verification",
    ),
    (
        "webchat.schema.config.oidc_issuer.description",
        "Pinned issuer whose access tokens may mint an identity-bound Direct Line token",
    ),
    (
        "webchat.schema.config.oidc_audience.title",
        "OIDC audience",
    ),
    (
        "webchat.schema.config.oidc_audience.description",
        "Expected aud claim on the access token. Defaults to webchat-gui",
    ),
    (
        "webchat.schema.config.oidc_required_scope.title",
        "OIDC required scope",
    ),
    (
        "webchat.schema.config.oidc_required_scope.description",
        "Scope the access token must carry. Defaults to greentic.webchat",
    ),
```

- [ ] **Step 3: Derive the issuer from the Greentic provider answer**

In both `lib.rs` files' `apply_answers`, after the `oauth_providers` composition block:

```rust
        if has("oauth_greentic_issuer") {
            merged.oidc_issuer = optional_string_from(&answers, "oauth_greentic_issuer");
        }
        if has("oauth_greentic_client_id") {
            merged.oidc_audience = optional_string_from(&answers, "oauth_greentic_client_id");
        }
```

`oidc_required_scope` is left unset; `handle_tokens` defaults it to `greentic.webchat`.

- [ ] **Step 4: Add the fields to both JSON schemas**

Add to the `properties` object of `config.schema.json` and `public.config.schema.json`:

```json
    "oidc_issuer": {
      "type": "string",
      "format": "uri",
      "description": "Pinned OIDC issuer for Direct Line chat-token verification."
    },
    "oidc_audience": {
      "type": "string",
      "description": "Expected aud claim on the access token."
    },
    "oidc_required_scope": {
      "type": "string",
      "description": "Scope the access token must carry."
    }
```

- [ ] **Step 5: Run the tests and regenerate fixtures**

Run:
```bash
cargo test -p messaging-provider-webchat --lib
cargo test -p messaging-provider-webchat-gui --lib
./tools/regenerate_registry_fixtures.sh
cargo test --workspace
```
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add components/ packs/messaging-webchat-gui/schemas/ tests/fixtures/registry/webchat/
git commit -m "feat(webchat): declare OIDC chat-token verification config"
```

---

### Task 13: Send the bearer from the browser

**Files:**
- Modify: `packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js` (`injectGuestIdIntoBody` ~1195, `/token` fetch branch ~1273)
- Modify: `tests/webchat-gui/fixtures/server.mjs`
- Test: `tests/webchat-gui/specs/fullscreen.spec.ts`

**Interfaces:**
- Consumes: `window.__GREENTIC_SSO_CLIENT__` from Task 3, and the
  `greentic_oauth_*` session keys the redirect fallback writes.
- Produces: `greenticAccessToken()` returning `Promise<string|null>`.

A user who fell back to the redirect flow (Task 3) has no SDK client, but does
have an access token in the session store. Both paths must reach the same mint
contract, so the bearer is sourced from whichever is present.

- [ ] **Step 1: Record the header in the fixture**

In `tests/webchat-gui/fixtures/server.mjs`, in the `/token` handler, capture the incoming authorization header so a test can read it back:

```js
  if (urlPath.endsWith('/token') || urlPath.endsWith('/v3/directline/tokens/generate')) {
    req.resume();
    lastTokenAuthorization = req.headers['authorization'] || null;
    sendJson(res, 200, {
      conversationId: 'webchat-gui-test',
      token: 'webchat-gui-test-token',
      expires_in: 1800,
    });
    return;
  }
```

Declare `let lastTokenAuthorization = null;` at module scope and expose it:

```js
  if (urlPath === '/mock-api/last-token-authorization') {
    sendJson(res, 200, { authorization: lastTokenAuthorization });
    return;
  }
```

- [ ] **Step 2: Write the failing test**

Append to `tests/webchat-gui/specs/fullscreen.spec.ts`:

```ts
test('an SSO session attaches a bearer to the token mint', async ({ page, request }) => {
  await page.addInitScript(() => {
    (window as any).__GREENTIC_SSO_CLIENT__ = {
      getAccessToken: () => Promise.resolve('fake-access-token'),
    };
  });
  await page.goto('/v1/web/webchat/default-plain-anon/');
  await expect
    .poll(async () => {
      const res = await request.get('/mock-api/last-token-authorization');
      return (await res.json()).authorization;
    })
    .toBe('Bearer fake-access-token');
});
```

- [ ] **Step 3: Run and confirm it fails**

Run: `npm run test:webchat-gui -- --grep "attaches a bearer"`
Expected: FAIL — `authorization` is `null`.

- [ ] **Step 4: Add the access-token accessor**

In `runtime-bootstrap.js`, immediately above the `window.fetch` interceptor,
insert:

```js
  function greenticAccessToken() {
    var client = window.__GREENTIC_SSO_CLIENT__;
    if (client && client.getAccessToken) {
      return client.getAccessToken().catch(function () { return null; });
    }
    try {
      // Only the redirect fallback puts a real bearer in the session store; the
      // SDK path stores a sentinel handle and serves tokens from the client.
      if (sessionStorage.getItem(oauthStorageKey('greentic_bearer')) === '1') {
        var session = getOAuthSession();
        if (session && session.token_handle) {
          return Promise.resolve(session.token_handle);
        }
      }
    } catch (_) {}
    return Promise.resolve(null);
  }
```

For this to cover the fallback, `greenticSsoRedirectFallback` in Task 3 must
leave a `greentic`-typed provider marker behind. In Task 3's
`greenticSsoRedirectFallback`, the object handed to `initiateOAuthFlow` carries
`type: 'oidc'` so the legacy flow runs; add one line before that call so the
marker survives the round trip:

```js
    try {
      sessionStorage.setItem(oauthStorageKey('greentic_fallback'), '1');
    } catch (_) {}
```

and in `handleOAuthCallback`'s `handleTokens`, after `saveOAuthSession(...)`,
stamp the provider type back:

```js
      try {
        if (sessionStorage.getItem(oauthStorageKey('greentic_fallback')) === '1') {
          sessionStorage.removeItem(oauthStorageKey('greentic_fallback'));
          sessionStorage.setItem(oauthStorageKey('greentic_bearer'), '1');
        }
      } catch (_) {}
```

Leave the `provider` marker alone here — `restoreGreenticSsoClient` in Task 3
keys off `provider.type === 'greentic'` to rebuild an SDK client, and the
fallback path has no SDK session for it to restore.

Note that `saveOAuthSession` stores `tokens.id_token || tokens.access_token`.
Change that expression to `tokens.access_token || tokens.id_token` so the value
the accessor returns is the bearer the mint expects. The id_token is not an API
credential and the mint would reject it on `aud`.

- [ ] **Step 5: Attach the bearer**

In `runtime-bootstrap.js`, replace the `/token` branch's `var nextInit = injectGuestIdIntoBody(init);` and the `originalFetch` call that follows with:

```js
      var nextInit = injectGuestIdIntoBody(init);
      return greenticAccessToken().then(function (accessToken) {
        if (accessToken) {
          nextInit.headers = nextInit.headers || {};
          nextInit.headers['Authorization'] = 'Bearer ' + accessToken;
        }
        return originalFetch(input, nextInit).then(function (response) {
```

Close the extra `then` at the end of that branch, keeping the existing 429, caching and `resetDirectLineAuthRetry` logic intact.

`injectGuestIdIntoBody` sets `nextInit.headers` to a plain object; confirm that before assigning into it, and if it uses a `Headers` instance, call `finalInit.headers.set('Authorization', …)` instead.

- [ ] **Step 6: Clear the new keys on logout**

`clearOAuthSession()` enumerates the keys it removes. Add the two new ones so a
logout cannot leave a stale bearer behind:

```js
      sessionStorage.removeItem(oauthStorageKey('greentic_bearer'));
      sessionStorage.removeItem(oauthStorageKey('greentic_fallback'));
```

- [ ] **Step 7: Run and confirm it passes**

Run: `npm run test:webchat-gui -- --grep "attaches a bearer"`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add packs/messaging-webchat-gui/assets/webchat-gui/runtime-bootstrap.js \
  tests/webchat-gui/fixtures/server.mjs tests/webchat-gui/specs/fullscreen.spec.ts
git commit -m "feat(webchat-gui): attach the SSO bearer to the Direct Line token mint"
```

---

### Task 14: Close out PR 2

- [ ] **Step 1: Rebuild the bundle and confirm no drift**

Run:
```bash
npm run build:sso-bundle
git diff --exit-code packs/messaging-webchat-gui/assets/webchat-gui/greentic-sso.js
```
Expected: no diff.

- [ ] **Step 2: Run the full local check**

Run: `./ci/local_check.sh`
Expected: every step passes.

- [ ] **Step 3: Run the full Playwright suite**

Run: `npm run test:webchat-gui`
Expected: PASS

- [ ] **Step 4: Update the pack lock**

Run:
```bash
./tools/build_packs.sh
python3 tools/update_packs_lock.py
```
Then commit `packs.lock.json` if it changed.

```bash
git add packs.lock.json
git commit -m "chore: refresh pack lock after SSO asset changes"
```

---

## Deployment prerequisites (outside this repo)

Per tenant, via the Greentic admin managed-SSO surface, before login works against a real issuer:

1. Register an OIDC public client — `webchat-gui` by platform convention, no client secret.
2. Register the exact absolute URL of `sso-callback.html` as a redirect URI. Exact string match; no wildcards.
3. Set `oidc_issuer` on the tenant's webchat provider so the mint trusts that issuer. Without it, PR 2's mint ignores the bearer and falls back to anonymous.

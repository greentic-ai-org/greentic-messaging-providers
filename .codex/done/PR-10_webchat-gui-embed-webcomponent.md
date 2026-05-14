Important: `skin` already means visual theme. Do not use `skin=embed_webcomponent`. Add `presentation_mode=embed_webcomponent` and keep `skin=default|3aigent|...`.

# PR-10: WebChat GUI Embed Web Component

## Goal

Add a new WebChat GUI presentation mode called `embed_webcomponent`.

Today `messaging-webchat-gui` can be configured as a hosted standalone WebChat GUI page. This PR adds an option for customers who already have their own website and only want to embed the deployed Greentic WebChat bundle inside that existing website.

The new mode must expose a framework-independent Web Component so it can be used from plain HTML, React, Vue, Angular, Svelte, Astro, Next.js, and similar host applications without requiring the host website to use Greentic's frontend framework.

## Current Audit Notes

- `packs/messaging-webchat-gui/assets/setup.yaml` currently defines Branding questions for:
  - `skin`: visual theme folder such as `default` or `3aigent`.
  - `nav_links`: the "Top-menu nav links" table shown in `gtc setup`.
- `setup.yaml` does not currently define `presentation_mode`.
- `nav_links` is currently always present in the setup spec and has no `visible_if`.
- `packs/messaging-webchat-gui/assets/webchat-gui/embed.js` already exists, but it is an IIFE-style global embed script using `window.greenticChatConfig` and `window.greenticChat`; it does not define `customElements.define("greentic-webchat", ...)`.
- `packs/messaging-webchat-gui/pack.yaml` already lists `assets/webchat-gui/embed.js` in `assets`.
- `packs/messaging-webchat-gui/pack.manifest.json` uses a static route:
  - `public_path: /v1/web/webchat/{tenant}`
  - `source_root: assets/webchat-gui`
  - `spa_fallback: index.html`
  This should make `/v1/web/webchat/{tenant}/embed.js` available if the static route serves files below `source_root`; verify this in implementation and document it.
- The WebChat GUI provider component currently reuses non-GUI WebChat modules by path:
  - `components/messaging-provider-webchat/src/config.rs`
  - `components/messaging-provider-webchat/src/describe.rs`
  - `components/messaging-provider-webchat/src/ops/mod.rs`
- Reusing the WebChat config/describe modules makes GUI-specific `presentation_mode`, `skin`, and `nav_links` behavior awkward and risks leaking GUI-only config into `messaging-provider-webchat`.
- Existing GUI config schemas are:
  - `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/public.config.schema.json`
  - `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/config.schema.json`
  They currently include backend fields like `enabled`, `public_base_url`, `mode`, `route`, `tenant_channel_id`, and `base_url`, but not `presentation_mode`, `skin`, or `nav_links`.
- Existing tests already check WebChat GUI pack metadata and static assets in `crates/provider-common/tests/pack_metadata.rs`, but they do not assert `embed.js` is included or defines a custom element.
- `tools/import_webchat_gui_assets.sh` imports SPA assets/config/i18n/js/skins and regenerates `index.html`/`404.html`; it intentionally preserves in-repo `runtime-bootstrap.js`. It currently does not copy or regenerate `embed.js`, which is good if `embed.js` remains maintained in this repo, but the script should explicitly preserve it.

## Scope

- Add `presentation_mode` setup/config support for WebChat GUI.
- Keep `skin` as visual theme selection only.
- Hide `nav_links` when `presentation_mode == embed_webcomponent`.
- Update WebChat GUI provider config/apply-answers behavior to persist and validate `presentation_mode`, `skin`, and `nav_links`.
- Replace or update `assets/webchat-gui/embed.js` with a framework-independent Web Component implementation.
- Preserve the existing standalone hosted page and static route behavior.
- Add docs and focused tests.

## Out Of Scope

- Do not change the non-GUI `messaging-provider-webchat` behavior unless a small shared helper is clearly needed.
- Do not require React/Vue/Angular runtime for the embed script.
- Do not remove standalone mode.
- Do not overload `skin` with embed behavior.
- Do not commit secrets or tenant-specific private config.

## Setup YAML Requirements

Update `packs/messaging-webchat-gui/assets/setup.yaml`:

1. Add a Branding question:

```yaml
- name: presentation_mode
  title: Presentation mode
  kind: string
  required: false
  default: standalone
  placeholder: "standalone | embed_webcomponent"
  help: "Choose standalone to host a full WebChat page, or embed_webcomponent to expose a framework-independent Web Component for an existing website."
  group: Branding
```

2. If `gtc setup` supports enum/select metadata, use it for allowed values:
   - `standalone`
   - `embed_webcomponent`

   If setup only supports string fields today, keep `kind: string` and validate allowed values in config/apply-answers.

3. Keep the existing `skin` question as a visual theme question.

4. Add this visibility condition to `nav_links`:

```yaml
visible_if:
  field: presentation_mode
  eq: "standalone"
```

5. Existing answer files without `presentation_mode` must continue to behave as `standalone`.

## Config And Apply-Answers Requirements

Update WebChat GUI config behavior so setup can persist:

```json
{
  "presentation_mode": "embed_webcomponent",
  "skin": "default",
  "nav_links": []
}
```

Rules:

- Default `presentation_mode` to `standalone`.
- Allowed values:
  - `standalone`
  - `embed_webcomponent`
- Reject unknown presentation modes.
- Existing configs without `presentation_mode` deserialize as `standalone`.
- Existing configs with `skin` keep working.
- When `presentation_mode == embed_webcomponent`, `nav_links` is optional and can be omitted or treated as empty.
- When `presentation_mode == standalone`, keep existing `nav_links` behavior.

Implementation guidance:

- Prefer adding GUI-specific modules:
  - `components/messaging-provider-webchat-gui/src/config.rs`
  - `components/messaging-provider-webchat-gui/src/describe.rs`
- Keep reusable Direct Line/ops behavior shared with `components/messaging-provider-webchat` only where it remains clean.
- Avoid adding GUI-only `presentation_mode`, `skin`, or `nav_links` fields to the non-GUI WebChat provider config unless there is a strong compatibility reason.
- Update `apply_answers_impl` in `components/messaging-provider-webchat-gui/src/lib.rs` to handle setup and upgrade modes for the new fields.
- Ensure `validate_config(_config_json)` uses real validation instead of always returning `{"ok": true}` if that is needed for schema/config correctness.

## Schema And Pack Metadata Requirements

Update:

- `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/public.config.schema.json`
- `packs/messaging-webchat-gui/schemas/messaging/webchat-gui/config.schema.json`
- `packs/messaging-webchat-gui/pack.yaml`
- `packs/messaging-webchat-gui/pack.manifest.json`

Schema fields:

- `presentation_mode`: string enum `["standalone", "embed_webcomponent"]`, default `standalone`.
- `skin`: string, default `default`.
- `nav_links`: array, default `[]`, optional.

Pack/static asset requirements:

- `assets/webchat-gui/embed.js` must be included in both pack YAML and manifest assets.
- Keep these existing assets working:
  - `assets/webchat-gui/index.html`
  - `assets/webchat-gui/404.html`
  - `assets/webchat-gui/runtime-bootstrap.js`
  - `assets/webchat-gui/config/product.json`
- Verify static route behavior:
  - existing standalone route remains `/v1/web/webchat/{tenant}`;
  - embed script is served stably as `/v1/web/webchat/{tenant}/embed.js`, or document the actual static file URL if the router behaves differently.
- Update `tools/import_webchat_gui_assets.sh` so importing frontend assets preserves or refreshes `embed.js` intentionally. Do not accidentally delete it when assets are imported.

## Web Component Requirements

Update `packs/messaging-webchat-gui/assets/webchat-gui/embed.js` so it defines a standards-based custom element:

```js
customElements.define("greentic-webchat", GreenticWebchatElement);
```

Target usage:

```html
<script
  type="module"
  src="https://YOUR-GREENTIC-HOST/v1/web/webchat/YOUR-TENANT/embed.js">
</script>

<greentic-webchat
  tenant="YOUR-TENANT"
  api-base="https://YOUR-GREENTIC-HOST/v1/messaging/webchat/YOUR-TENANT"
  skin="default"
  launcher="true">
</greentic-webchat>
```

Implementation requirements:

- Use plain browser Web Component APIs.
- No required React/Vue/Angular runtime.
- Use Shadow DOM unless there is a strong reason not to.
- Avoid leaking CSS to the host page.
- Avoid polluting globals except the custom element definition.
- Support multiple instances on a page if practical.
- Do not render or require top menu / top navigation links in embedded mode.
- Do not depend on `window.greenticChatConfig` as the primary API. If legacy global config remains, keep it as backwards-compatible sugar only.

Minimum supported attributes/properties:

- `tenant`
- `api-base`
- `public-base-url`
- `skin`
- `launcher`
- `open`
- `locale`
- `title`

Minimum DOM events:

- `greentic-webchat-ready`
- `greentic-webchat-open`
- `greentic-webchat-close`
- `greentic-webchat-error`

Document any final attribute/property naming if implementation chooses slightly different names to align with existing frontend conventions.

## Documentation Requirements

Add:

- `docs/guides/webchat-gui-embed-webcomponent.md`

Update:

- `packs/messaging-webchat-gui/README.md`

Docs must explain:

1. Modes:
   - `standalone`: Greentic hosts a full WebChat GUI page, including page shell/header/top nav.
   - `embed_webcomponent`: Greentic serves a Web Component for an existing customer website; customer website owns header, navigation, layout, and surrounding page.
2. How to select embedded mode in setup.
3. Example answers file:

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

4. Mention that `nav_links` is not needed and should not be prompted when `presentation_mode` is `embed_webcomponent`.
5. Plain HTML example with `<script type="module">` and `<greentic-webchat>`.
6. React example using dynamic `import()` and JSX.
7. TypeScript JSX declaration for `"greentic-webchat"`.
8. Vue example using `onMounted`.
9. Angular / generic module-loader note.
10. Attribute and event reference.
11. Security notes:
    - Do not put secrets in HTML.
    - The Web Component should use public/token endpoints only.
    - Configure OAuth/Direct Line token handling through setup.
    - Consider CSP `script-src` and `connect-src`.
    - Consider allowed parent origins if implemented.
    - Use HTTPS in production.
    - Explain CORS implications when the customer website and Greentic deployment are on different origins.
12. Troubleshooting:
    - `embed.js` returns 404.
    - custom element not defined.
    - CSP blocks script.
    - CORS blocks API calls.
    - wrong tenant.
    - token/auth failures.
    - skin not found.
    - chat opens but messages do not send.

## Tests

Add or update tests for:

- `setup.yaml` contains `presentation_mode`.
- `presentation_mode` defaults to `standalone`.
- `nav_links` has `visible_if` so it only appears for `standalone`.
- Config/schema accepts:
  - missing `presentation_mode`,
  - `standalone`,
  - `embed_webcomponent`.
- Config/schema rejects unknown presentation modes.
- `apply_answers` persists `presentation_mode`.
- `apply_answers` with `embed_webcomponent` does not require `nav_links`.
- Pack metadata includes `assets/webchat-gui/embed.js`.
- `embed.js` defines `greentic-webchat`.

Suggested locations:

- Rust config/apply-answer tests near `components/messaging-provider-webchat-gui/src/config.rs` and `src/lib.rs`.
- Pack/schema/static asset tests in `crates/provider-common/tests/pack_metadata.rs` or a focused neighboring test.
- Setup YAML lint in `crates/provider-common/tests/spec_lints.rs` or a focused neighboring test.
- If JS tests already exist, add a browser-ish test for custom element registration. If not, add a lightweight static check that verifies `customElements.define("greentic-webchat", ...)` or equivalent.

## Verification Commands

Run relevant checks:

```bash
cargo fmt --check
cargo test --workspace
./tools/build_components/messaging-provider-webchat-gui.sh
PACK_FILTER=messaging-webchat-gui ./ci/steps/11_build_packs.sh
```

Adjust commands if newer per-provider build/test scripts from earlier PRs are available by the time this PR is implemented.

## Backwards Compatibility

Do not break existing standalone deployments.

Required compatibility:

- Existing answer files with only `skin` continue to work.
- Existing answer files without `presentation_mode` behave as `standalone`.
- Existing `nav_links` keep working in standalone mode.
- Existing `/v1/web/webchat/{tenant}` route keeps working.
- Existing skins such as `default` and `3aigent` keep working.
- Existing assets listed in pack metadata continue to be present.
- If the legacy global `window.greenticChatConfig` embed API is still used by customers, either preserve it or document the migration to `<greentic-webchat>`.

## Acceptance Criteria

- `gtc setup` shows a `presentation_mode` field for WebChat GUI.
- Selecting `embed_webcomponent` hides/removes the "Top-menu nav links" table.
- Selecting `standalone` keeps the current Branding / Skin / Top-menu nav links behavior.
- The pack includes a framework-independent `embed.js`.
- The embed script defines a custom element usable from plain HTML.
- Documentation explains how to embed into existing websites.
- Tests cover setup YAML visibility, config validation, apply answers, pack asset inclusion, and basic embed asset presence.
- Existing hosted standalone WebChat GUI behavior is preserved.

## Review Notes

- This is a cross-boundary PR: setup spec, provider config, static asset, pack metadata, docs, and tests all need to agree.
- Keep the Web Component API small and stable. It is an external customer-facing contract.
- Be especially careful not to turn `skin` into a mode switch; it is only the visual theme folder.

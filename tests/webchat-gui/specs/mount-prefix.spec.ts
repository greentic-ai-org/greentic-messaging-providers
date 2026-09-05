import { expect, test } from '@playwright/test';
import { WebChatGuiPage, tenantName } from '../pages/webchatGuiPage';

/**
 * Serving the GUI behind a reverse-proxy path prefix.
 *
 * The Greentic Designer proxies a running operator environment at
 * /api/operator-env/<id>/proxy/<path>. It is a deliberate byte passthrough: it
 * rewrites no HTML, injects no <base href>, sets no prefix header and follows
 * no redirects. So the page's own URL is the ONLY evidence the prefix exists.
 *
 * runtime-bootstrap.js used to extract the tenant with an unanchored regex —
 * which works behind a prefix — and then rebuild its base paths from the site
 * root, discarding it. The HTML shell still rendered (index.html uses relative
 * ./ paths), but every runtime fetch went to the wrong path.
 *
 * The i18n fetch is the one that matters most, because the failure is silent:
 * the Designer answers an unknown path from its SPA catch-all with 200 OK and
 * its own index.html, so res.ok is true, the fallback branch never fires,
 * res.json() rejects, the string table stays empty, and the UI renders raw
 * keys like `status.experienceUnavailable`.
 *
 * These tests therefore assert on the URLs the page EMITS, not on whether the
 * fixture happened to answer them — the fixture serves both shapes, so a
 * "the page loaded" assertion would pass with the bug still present.
 */

const MOUNT_PREFIX = '/api/operator-env/abc/proxy';

// Its own tenant: the fixture records the last /token Authorization header per
// tenant, so sharing `default-plain-anon` with fullscreen.spec.ts would
// overwrite the recording that spec's SSO-bearer assertion polls for.
const PROXIED = { skin: 'default', nav: false, login: false, variant: 'proxied' } as const;
const ROOTED = { skin: 'default', nav: false, login: false, variant: 'rooted' } as const;

// Every path the runtime builds from its own base. A leading-slash request to
// any of these that does not carry the prefix is the bug.
const RUNTIME_PATH_PATTERN = /\/(?:i18n\/|config\/tenants\/|skins\/|v1\/messaging\/webchat\/)/;

test.describe('reverse-proxy mount prefix', () => {
  test('every runtime URL is built under the proxy prefix', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.installMockWebChat();

    const runtimePaths: string[] = [];
    page.on('request', (request) => {
      const { pathname } = new URL(request.url());
      if (RUNTIME_PATH_PATTERN.test(pathname)) runtimePaths.push(pathname);
    });

    await webchat.openFullscreen({ ...PROXIED, mountPrefix: MOUNT_PREFIX });
    await webchat.expectChatReady();

    const tenant = tenantName(PROXIED);

    // The bootstrap's single lever: everything else is derived from it.
    const basePath = await page.evaluate(
      () => (window as unknown as { __BASE_PATH__?: string }).__BASE_PATH__ ?? null,
    );
    expect(basePath).toBe(`${MOUNT_PREFIX}/v1/web/webchat/${tenant}/`);

    const mountPrefix = await page.evaluate(
      () => (window as unknown as { __GREENTIC_MOUNT_PREFIX__?: string }).__GREENTIC_MOUNT_PREFIX__ ?? null,
    );
    expect(mountPrefix).toBe(MOUNT_PREFIX);

    // The Direct Line endpoints the SPA is handed.
    const backendBase = await page.evaluate(
      () => (window as unknown as { __WEBCHAT_BACKEND_BASE__?: string }).__WEBCHAT_BACKEND_BASE__ ?? null,
    );
    expect(backendBase).toBe(
      `${new URL(page.url()).origin}${MOUNT_PREFIX}/v1/messaging/webchat/${tenant}`,
    );

    // Nothing the page fetched escaped the prefix.
    expect(runtimePaths.length, 'no runtime fetches observed at all').toBeGreaterThan(0);
    expect(
      runtimePaths.filter((pathname) => !pathname.startsWith(`${MOUNT_PREFIX}/`)),
      'these runtime URLs were built from the site root instead of the mount prefix',
    ).toEqual([]);

    // The i18n catalog actually parsed: an empty string table renders raw keys.
    expect(runtimePaths.some((pathname) => pathname.includes('/i18n/'))).toBe(true);
  });

  test('the site root still builds root-relative URLs', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.installMockWebChat();

    await webchat.openFullscreen(ROOTED);
    await webchat.expectChatReady();

    const tenant = tenantName(ROOTED);

    expect(
      await page.evaluate(() => (window as unknown as { __BASE_PATH__?: string }).__BASE_PATH__ ?? null),
    ).toBe(`/v1/web/webchat/${tenant}/`);
    expect(
      await page.evaluate(
        () => (window as unknown as { __GREENTIC_MOUNT_PREFIX__?: string }).__GREENTIC_MOUNT_PREFIX__ ?? null,
      ),
    ).toBe('');
  });

  test('prefix derivation covers the shapes a proxy can produce', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.installMockWebChat();
    await webchat.openFullscreen(ROOTED);

    const derived = await page.evaluate(() => {
      const from = (window as unknown as {
        __greenticMountPrefixFrom__: (pathname: string | null) => string;
      }).__greenticMountPrefixFrom__;
      return {
        root: from('/v1/web/webchat/default/'),
        rootWithBundle: from('/v1/web/webchat/default/worker/'),
        // The exact URL the Designer's operator-environment conduit serves.
        proxied: from('/api/operator-env/abc/proxy/v1/web/webchat/default/worker/'),
        proxiedNoBundle: from('/api/operator-env/abc/proxy/v1/web/webchat/default/'),
        // A prefix that already ends in a slash must not produce '//'.
        doubleSlash: from('/proxy//v1/web/webchat/default/'),
        leadingDoubleSlash: from('//v1/web/webchat/default/'),
        // The backend namespace resolves the same way.
        messaging: from('/a/b/v1/messaging/webchat/tenant/token'),
        // No route marker at all: unchanged, root-relative behaviour.
        unrelated: from('/greentic-webchat/index.html'),
        empty: from(''),
        nullish: from(null),
      };
    });

    expect(derived).toEqual({
      root: '',
      rootWithBundle: '',
      proxied: '/api/operator-env/abc/proxy',
      proxiedNoBundle: '/api/operator-env/abc/proxy',
      doubleSlash: '/proxy',
      leadingDoubleSlash: '',
      messaging: '/a/b',
      unrelated: '',
      empty: '',
      nullish: '',
    });
  });
});

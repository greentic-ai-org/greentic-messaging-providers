import { expect, test } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { WebChatGuiPage } from '../pages/webchatGuiPage';

/**
 * Custom-skin selection.
 *
 * The webchat-gui SPA picks which skins/<name>/ folder to load from the tenant
 * config's `legacy_skin` field. But greentic-setup's sync_skin writes the
 * operator's choice into the modern `skin` field only — `legacy_skin` keeps the
 * default.json scaffold value ("default"). So on real gtc a tenant set up with
 * a custom skin ends up { skin: "3aigent", legacy_skin: "default" } and the SPA
 * loads the default skin instead. Nav links still work (plain tenant-config
 * data), which is why the symptom is "nav links OK, custom skin missing".
 *
 * The fixture mirrors gtc (legacy_skin = "default"); runtime-bootstrap.js
 * reconciles `legacy_skin` to `skin` so the chosen skin actually renders.
 */
test.describe('custom skin selection', () => {
  test('3aigent adaptive card actions use default non-stretched sizing', async () => {
    for (const file of ['hostconfig.json', 'hostconfig-light.json', 'hostconfig-dark.json']) {
      const hostConfigPath = path.resolve(
        process.cwd(),
        'packs/messaging-webchat-gui/assets/webchat-gui/skins/3aigent/webchat',
        file,
      );
      const hostConfig = JSON.parse(fs.readFileSync(hostConfigPath, 'utf8'));
      expect(hostConfig.actions?.actionAlignment, file).toBe('left');
    }
  });

  test('3aigent skin loads, not the default fallback', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.installMockWebChat();

    const skinJsonFolders: string[] = [];
    const skinAssetFailures: string[] = [];
    page.on('response', (response) => {
      const { pathname } = new URL(response.url());
      const match = pathname.match(/\/skins\/([^/]+)\/skin\.json$/);
      if (match) skinJsonFolders.push(match[1]);
      if (pathname.includes('/skins/') && response.status() >= 400) {
        skinAssetFailures.push(`${response.status()} ${pathname}`);
      }
    });

    await webchat.openFullscreen({ skin: '3aigent', nav: true, login: false });
    await webchat.expectChatReady();

    // The decisive check: the skin the SPA actually loaded.
    const loadedSkin = await page.evaluate(
      () => (window as unknown as { __SKIN__?: { tenant?: string; brand?: { name?: string } } }).__SKIN__ ?? null,
    );
    expect(loadedSkin, 'no skin loaded into window.__SKIN__').not.toBeNull();
    expect(
      loadedSkin?.tenant,
      `expected the 3aigent skin, but loaded: ${JSON.stringify(loadedSkin)}`,
    ).toBe('3aigent');

    expect(
      skinJsonFolders,
      'SPA should request the 3aigent skin folder, not fall back to default',
    ).toContain('3aigent');
    expect(skinJsonFolders).not.toContain('default');

    // Skin assets must also resolve (gtc has no root /skins/ route).
    expect(
      skinAssetFailures,
      `skin assets failed to load:\n${skinAssetFailures.join('\n')}`,
    ).toEqual([]);
    await webchat.expectNoBrokenImages();
  });
});

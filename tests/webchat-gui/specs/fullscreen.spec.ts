import { expect, test, type Page } from '@playwright/test';
import { WebChatGuiPage, type Skin } from '../pages/webchatGuiPage';

const skins: Skin[] = ['default', '3aigent'];

test.beforeEach(async ({ page }) => {
  const webchat = new WebChatGuiPage(page);
  await webchat.installMockWebChat();
  const consoleErrors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('response', (response) => {
    if (response.status() >= 400) consoleErrors.push(`${response.status()} ${response.url()}`);
  });
  page.on('pageerror', (error) => consoleErrors.push(error.message));
  await page.addInitScript(() => {
    window.addEventListener('beforeunload', () => sessionStorage.clear());
  });
  await page.exposeFunction('__webchatConsoleErrors', () => consoleErrors);
});

async function expectNoConsoleErrors(page: Page) {
  const errors = await page.evaluate(async () => {
    return await (window as unknown as { __webchatConsoleErrors: () => Promise<string[]> }).__webchatConsoleErrors();
  });
  expect(errors.filter((line) => !line.includes('favicon'))).toEqual([]);
}

test.describe('full-screen WebChat', () => {
  for (const skin of skins) {
    test(`${skin} skin loads anonymously with chat input`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openFullscreen({ skin, nav: false, login: false });

      await expect(page.locator('.topbar__title')).toContainText(skin === '3aigent' ? '3AIgent' : 'Greentic');
      await webchat.expectChatReady();
      await webchat.sendMessage('Hello');
      await webchat.expectNoBrokenImages();
      await webchat.expectNoHorizontalOverflow();
      await expectNoConsoleErrors(page);
    });
  }

  test('navigation links are visible when configured', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: true, login: false });

    await expect(page.getByRole('navigation', { name: 'Site navigation' }).getByRole('link', { name: 'Docs' })).toBeVisible();
    await expect(page.getByRole('navigation', { name: 'Site navigation' }).getByRole('link', { name: 'Playground' })).toBeVisible();
    await webchat.expectChatReady();
  });

  test('navigation links are hidden when disabled', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: false });

    await expect(page.getByRole('navigation', { name: 'Site navigation' }).getByRole('link', { name: 'Docs' })).toHaveCount(0);
    await webchat.expectChatReady();
  });

  test('configured login gate appears and transitions to chat', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: true });

    await expect(page.getByText('Sign in to start chatting')).toBeVisible();
    const loginButton = page.getByRole('button', { name: /test login/i });
    await expect(loginButton).toBeVisible();
    await loginButton.click();
    await webchat.expectChatReady();
    await expect(page.evaluate(() => sessionStorage.getItem('greentic_oauth_token_handle'))).resolves.toBe('guest');
  });

  test('tenant-config login fallback appears when auth config endpoint is unavailable', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await page.goto('/v1/web/webchat/tenant-config-login/?tenant=tenant-config-login&loginRequired=1');

    await expect(page.getByRole('heading', { name: /sign in to greentic/i })).toBeVisible();
    await page.getByRole('button', { name: /continue as guest/i }).click();
    await webchat.expectChatReady();
    await expect(page.evaluate(() => {
      const raw = localStorage.getItem('webchat_auth_session');
      return raw ? JSON.parse(raw).isAuthenticated === true : false;
    })).resolves.toBe(true);
  });

  test('anonymous mode skips the login page', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: false });

    await expect(page.getByText('Sign in to start chatting')).toHaveCount(0);
    await webchat.expectChatReady();
  });

  test('adaptive cards default to 70 percent width', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: false });
    await webchat.expectChatReady();

    await expect(page.getByTestId('adaptive-card')).toBeVisible();
    await expect.poll(async () => page.getByTestId('adaptive-card').evaluate((card) => {
      const bubble = card.closest('.webchat__bubble__content');
      const transcript = card.closest('[data-testid="webchat-transcript"]');
      if (!bubble || !transcript) return 0;
      return bubble.getBoundingClientRect().width / transcript.getBoundingClientRect().width;
    })).toBeCloseTo(0.7, 1);
  });

  test('3aigent light mode keeps adaptive card text readable', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await page.addInitScript(() => sessionStorage.setItem('greentic-theme', 'light'));
    await webchat.openFullscreen({ skin: '3aigent', nav: false, login: false });
    await webchat.expectChatReady();

    const color = await page.getByTestId('adaptive-card-title').evaluate((element) => {
      return window.getComputedStyle(element).color;
    });
    const channels = color.match(/\d+(\.\d+)?/g)?.slice(0, 3).map(Number) ?? [];
    expect(channels).toHaveLength(3);
    expect(Math.max(...channels)).toBeLessThan(100);
  });

  test('default skin uses Greentic green adaptive card action styling', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: false });
    await webchat.expectChatReady();

    const action = page.getByTestId('adaptive-card-action');
    await expect(action).toBeVisible();
    const styles = await action.evaluate((element) => {
      const computed = window.getComputedStyle(element);
      return {
        backgroundColor: computed.backgroundColor,
        borderColor: computed.borderTopColor,
        color: computed.color,
      };
    });
    expect(styles.backgroundColor).toBe('rgba(0, 0, 0, 0)');
    expect(styles.borderColor).toBe('rgb(5, 150, 105)');
    expect(styles.color).toBe('rgb(5, 150, 105)');
  });

  for (const skin of skins) {
    test(`${skin} skin differentiates adaptive card action styles`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openFullscreen({ skin, nav: false, login: false });
      await webchat.expectChatReady();

      const defaultAction = page.getByTestId('adaptive-card-action');
      const positiveAction = page.getByTestId('adaptive-card-action-positive');
      const destructiveAction = page.getByTestId('adaptive-card-action-destructive');
      await expect(defaultAction).toBeVisible();
      await expect(positiveAction).toBeVisible();
      await expect(destructiveAction).toBeVisible();

      const styles = await Promise.all(
        [defaultAction, positiveAction, destructiveAction].map((locator) => locator.evaluate((element) => {
          const computed = window.getComputedStyle(element);
          return {
            backgroundColor: computed.backgroundColor,
            borderColor: computed.borderTopColor,
            color: computed.color,
          };
        }))
      );
      const [defaultStyles, positiveStyles, destructiveStyles] = styles;

      expect(defaultStyles.backgroundColor).toBe('rgba(0, 0, 0, 0)');
      expect(positiveStyles.backgroundColor).not.toBe(defaultStyles.backgroundColor);
      expect(positiveStyles.color).toBe('rgb(255, 255, 255)');
      expect(destructiveStyles.backgroundColor).toBe('rgb(220, 38, 38)');
      expect(destructiveStyles.borderColor).toBe('rgb(252, 165, 165)');
      expect(destructiveStyles.color).toBe('rgb(255, 255, 255)');
    });
  }

  test('default dark mode keeps adaptive card action borders green and visible', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await page.addInitScript(() => sessionStorage.setItem('greentic-theme', 'dark'));
    await webchat.openFullscreen({ skin: 'default', nav: false, login: false });
    await webchat.expectChatReady();

    await expect(page.getByTestId('adaptive-card-action')).toBeVisible();
    const borderColor = await page.getByTestId('adaptive-card-action').evaluate((element) => {
      return window.getComputedStyle(element).borderTopColor;
    });
    expect(borderColor).toBe('rgb(52, 211, 153)');
  });

  for (const skin of skins) {
    test(`${skin} skin uses brand color for adaptive card links`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openFullscreen({ skin, nav: false, login: false });
      await webchat.expectChatReady();

      const link = page.getByTestId('adaptive-card-link');
      const email = page.getByTestId('adaptive-card-email');
      await expect(link).toHaveAttribute('href', 'https://adaptivecards.io/');
      await expect(email).toHaveAttribute('href', 'mailto:support@greentic.ai');

      await expect.poll(async () => link.evaluate((node, activeSkin) => {
        const rootStyle = getComputedStyle(document.documentElement);
        const theme = document.documentElement.getAttribute('data-theme');
        const expectedVariable = activeSkin === '3aigent' && theme !== 'light'
          ? '--brand-light'
          : activeSkin === '3aigent'
            ? '--brand-dark'
            : '--brand';
        const probe = document.createElement('span');
        probe.style.color = rootStyle.getPropertyValue(expectedVariable).trim();
        document.body.appendChild(probe);
        const expected = getComputedStyle(probe).color;
        probe.remove();
        return getComputedStyle(node).color === expected;
      }, skin)).toBe(true);
    });

    test(`${skin} skin shows adaptive card action spinner while pending`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openFullscreen({ skin, nav: false, login: false });
      await webchat.expectChatReady();

      const action = page.getByTestId('adaptive-card-action');
      await expect(action).toBeVisible();
      await action.click();

      await expect(action).toBeDisabled();
      await expect(action).toHaveAttribute('aria-busy', 'true');
      await expect.poll(async () => action.evaluate((button) => {
        const before = getComputedStyle(button, '::before');
        return {
          animationName: before.animationName,
          content: before.content,
          cursor: getComputedStyle(button).cursor,
        };
      })).toMatchObject({
        animationName: 'ac-action-spin',
        content: '""',
        cursor: 'wait',
      });
    });
  }

  test('missing tenant config does not invent a login page', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    const tokenRequests: string[] = [];
    page.on('request', request => {
      const url = request.url();
      if (url.includes('/v1/messaging/webchat/') && url.includes('/token')) {
        tokenRequests.push(url);
      }
    });
    await page.goto('/v1/web/webchat/missing-config-anon/?tenant=missing-config-anon');

    await expect(page.getByText('Sign in to start chatting')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /sign in/i })).toHaveCount(0);
    await webchat.expectChatReady();
    expect(tokenRequests.some(url => url.includes('/v1/messaging/webchat/missing-config-anon/token'))).toBe(true);
    expect(tokenRequests.some(url => url.includes('/v1/messaging/webchat/greentic/token'))).toBe(false);
  });

  test('login callback error shows a clear failure state', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: false, login: true });
    await page.goto(`${webchat.fullscreenUrl({ skin: 'default', nav: false, login: true })}&error=access_denied&error_description=Denied`);

    await expect(page.getByRole('heading', { name: 'Something went wrong' })).toBeVisible();
    await expect(page.getByText(/Authentication failed: Denied/)).toBeVisible();
  });

  test('mobile viewport keeps chat input usable @mobile', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: '3aigent', nav: true, login: false });

    await webchat.expectChatReady();
    await expect(webchat.chatInput()).toBeInViewport();
    await webchat.expectNoHorizontalOverflow();
  });

  test('sso callback page loads the SDK and exposes the completer', async ({ page }) => {
    await page.goto('/v1/web/webchat/default-plain-anon/sso-callback.html');
    const hasCompleter = await page.evaluate(
      () => typeof (window as any).GreenticSso?.completeCallbackFromPopup === 'function',
    );
    expect(hasCompleter).toBe(true);
  });

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
});

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

    await expect(page.getByTestId('adaptive-card-action')).toBeVisible();
    const styles = await page.getByTestId('adaptive-card-action').evaluate((element) => {
      const computed = window.getComputedStyle(element);
      return {
        backgroundColor: computed.backgroundColor,
        borderColor: computed.borderTopColor,
        color: computed.color,
      };
    });
    expect(styles.backgroundColor).toBe('rgb(236, 253, 245)');
    expect(styles.borderColor).toBe('rgba(5, 150, 105, 0.35)');
    expect(styles.color).toBe('rgb(6, 78, 59)');
  });

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
    test(`${skin} skin shows adaptive card action pending spinner`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openFullscreen({ skin, nav: false, login: false });
      await webchat.expectChatReady();

      const action = page.getByTestId('adaptive-card-action');
      await action.click();
      await expect(action).toBeDisabled();
      const spinner = await action.evaluate((element) => {
        const styles = window.getComputedStyle(element, '::after');
        return {
          content: styles.content,
          animationName: styles.animationName,
          borderTopColor: styles.borderTopColor,
        };
      });
      expect(spinner.content).toBe('""');
      expect(spinner.animationName).toBe('greentic-ac-action-spin');
      expect(spinner.borderTopColor).toBe('rgba(0, 0, 0, 0)');
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
});

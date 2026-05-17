import { expect, test } from '@playwright/test';
import { WebChatGuiPage, type Skin } from '../pages/webchatGuiPage';

test.beforeEach(async ({ page }) => {
  await new WebChatGuiPage(page).installMockWebChat();
});

test.describe('embedded WebChat modes', () => {
  test('native in-page embed stays inside the host page', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: 'default', render: 'native', mode: 'inline', nav: true, login: false });

    await expect(page.getByTestId('host-content')).toBeVisible();
    await webchat.expectChatReady();
    await webchat.sendMessage('Hello native inline');
    await expect(webchat.embeddedElement()).toBeInViewport();
    await webchat.expectNoHorizontalOverflow();
  });

  test('native popup launcher opens, closes, and supports keyboard focus', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: '3aigent', render: 'native', mode: 'launcher', nav: false, login: false });

    const launcher = webchat.launcherButton();
    await expect(launcher).toBeVisible();
    await launcher.focus();
    await expect(launcher).toBeFocused();
    await page.keyboard.press('Enter');
    await webchat.expectChatReady();
    await page.getByTestId('host-close-button').click();
    await expect(launcher).toHaveAttribute('aria-expanded', 'false');
  });

  test('iframe in-page embed loads a usable chat frame', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: 'default', render: 'iframe', mode: 'inline', nav: false, login: false });

    const frame = webchat.iframeChat();
    await webchat.expectChatReady(frame);
    await webchat.sendMessage('Hello iframe inline', frame);
    await expect(webchat.embeddedElement().locator('iframe')).toBeVisible();
  });

  test('iframe launcher opens and does not take over the host page', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: '3aigent', render: 'iframe', mode: 'launcher', nav: true, login: false });

    await expect(page.getByTestId('host-content')).toBeVisible();
    await webchat.launcherButton().click();
    await webchat.expectChatReady(webchat.iframeChat());
    await expect(page.getByTestId('host-content')).toBeVisible();
    await webchat.expectNoHorizontalOverflow();
  });

  for (const skin of ['default', '3aigent'] as Skin[]) {
    test(`iframe inline applies ${skin} skin CSS`, async ({ page }) => {
      const webchat = new WebChatGuiPage(page);
      await webchat.openHost({ skin, render: 'iframe', mode: 'inline', nav: false, login: false });

      const frame = webchat.iframeChat();
      await webchat.expectChatReady(frame);
      await expect.poll(async () => frame.locator('body').evaluate(() => (window as any).__SKIN__?.brand?.name)).toBe(skin === '3aigent' ? '3AIgent' : 'Greentic');
    });
  }

  test('login gate appears in embedded iframe and then opens chat', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: 'default', render: 'iframe', mode: 'inline', nav: false, login: true });

    const frame = webchat.iframeChat();
    await expect(frame.getByText('Sign in to start chatting')).toBeVisible();
    await frame.getByRole('button', { name: /test login/i }).click();
    await webchat.expectChatReady(frame);
  });

  test('popup mode renders on screen at mobile size', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: 'default', render: 'iframe', mode: 'launcher', nav: false, login: false });

    await webchat.launcherButton().click();
    const iframe = webchat.embeddedElement().locator('iframe');
    await expect(iframe).toBeInViewport();
    await webchat.expectChatReady(webchat.iframeChat());
  });
});

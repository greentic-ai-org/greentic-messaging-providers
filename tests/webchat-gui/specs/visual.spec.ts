import { expect, test } from '@playwright/test';
import { WebChatGuiPage } from '../pages/webchatGuiPage';

test.beforeEach(async ({ page }) => {
  await new WebChatGuiPage(page).installMockWebChat();
});

test.describe('visual regression snapshots @visual', () => {
  test.skip(!process.env.WEBCHAT_GUI_VISUAL, 'Visual snapshots are opt-in; run npm run test:webchat-gui:update-snapshots.');

  test('full-screen default skin', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: 'default', nav: true, login: false });
    await webchat.expectChatReady();
    await expect(page).toHaveScreenshot('fullscreen-default.png', { fullPage: true });
  });

  test('full-screen 3aigent skin', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openFullscreen({ skin: '3aigent', nav: true, login: false });
    await webchat.expectChatReady();
    await expect(page).toHaveScreenshot('fullscreen-3aigent.png', { fullPage: true });
  });

  test('native popup closed and opened', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: 'default', render: 'native', mode: 'launcher', nav: false, login: false });
    await expect(page).toHaveScreenshot('native-popup-closed.png', { fullPage: true });
    await webchat.launcherButton().click();
    await webchat.expectChatReady();
    await expect(page).toHaveScreenshot('native-popup-opened.png', { fullPage: true });
  });

  test('iframe inline and login page', async ({ page }) => {
    const webchat = new WebChatGuiPage(page);
    await webchat.openHost({ skin: '3aigent', render: 'iframe', mode: 'inline', nav: false, login: false });
    await webchat.expectChatReady(webchat.iframeChat());
    await expect(page).toHaveScreenshot('iframe-inline.png', { fullPage: true });

    await webchat.openFullscreen({ skin: 'default', nav: false, login: true });
    await page.getByText('Sign in to start chatting').waitFor();
    await expect(page).toHaveScreenshot('login-page.png', { fullPage: true });
  });
});

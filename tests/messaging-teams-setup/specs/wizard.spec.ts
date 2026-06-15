import { expect, test, type APIRequestContext, type Page } from '@playwright/test';

async function progressText(page: Page) {
  return page.locator('greentic-teams-setup-v4').getByText(/\d+ of 6 complete/).textContent();
}

async function expectNoRegression(page: Page, expected: string) {
  await expect(page.getByText(expected)).toBeVisible();
  await page.waitForTimeout(1_500);
  await expect(page.getByText(expected)).toBeVisible();
  expect(await progressText(page)).toContain(expected);
}

async function clickWithOptionalPopup(page: Page, name: RegExp) {
  const popupPromise = page.waitForEvent('popup', { timeout: 2_000 }).catch(() => null);
  await page.getByRole('button', { name }).click();
  const popup = await popupPromise;
  if (popup) await popup.close();
}

test.beforeEach(async ({ page, request }) => {
  await request.get('/reset');
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('response', (response) => {
    if (response.status() >= 400 && !response.url().includes('/favicon')) {
      errors.push(`${response.status()} ${response.url()}`);
    }
  });
  await page.exposeFunction('__setupErrors', () => errors);
});

async function resetScenario(request: APIRequestContext, scenario: string) {
  await request.get(`/reset?scenario=${encodeURIComponent(scenario)}`);
}

async function nextRequestCount(request: APIRequestContext) {
  const payload = await request.get('/requests').then((res) => res.json());
  return payload.requests.filter((entry: { path: string }) => entry.path === '/api/next').length;
}

async function clickContinueAndWaitForNextRequest(page: Page, request: APIRequestContext) {
  const before = await nextRequestCount(request);
  const button = page.getByRole('button', { name: 'Continue setup' });
  await expect(button).toBeEnabled();
  await button.click();
  await expect.poll(() => nextRequestCount(request)).toBeGreaterThan(before);
}

async function startThroughBotIdentity(page: Page, request: APIRequestContext) {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Teams setup' })).toBeVisible();
  await expect(page.locator('[data-role="overall"]')).toHaveText('Ready');
  await clickContinueAndWaitForNextRequest(page, request);
  await expect(page.getByText('Microsoft sign-in required')).toBeVisible();
  await expect(page.locator('span.code', { hasText: /GRAPH-CODE-/ })).toBeVisible();
  await clickWithOptionalPopup(page, /Open Microsoft device login/);
  await expect(page.getByText('2 of 6 complete')).toBeVisible();
}

async function startThroughBotFrameworkRegistration(page: Page, request: APIRequestContext) {
  await startThroughBotIdentity(page, request);
  await clickContinueAndWaitForNextRequest(page, request);
  await expect(page.getByText('3 of 6 complete')).toBeVisible();
}

test('Teams setup web component completes against a fake backend without progress regression', async ({ page, request }) => {
  await page.goto('/');

  await expect(page.getByRole('heading', { name: 'Teams setup' })).toBeVisible();
  await expect(page.getByText('0 of 6 complete')).toBeVisible();

  await page.getByText('Advanced configuration').click();
  await page.getByLabel('Bot display name').fill('Custom Greentic Bot');
  await page.getByLabel('Public base URL').fill('https://custom-runtime.example.test');
  await page.getByRole('button', { name: 'Save configuration' }).click();
  await expect(page.getByText('configuration saved')).toBeVisible();

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('Microsoft sign-in required')).toBeVisible();
  await expect(page.locator('span.code', { hasText: 'GRAPH-CODE-1' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Open Microsoft device login' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Refresh code' })).toBeVisible();

  await page.getByRole('button', { name: 'Refresh code' }).click();
  await expect(page.locator('span.code', { hasText: 'GRAPH-CODE-2' })).toBeVisible();

  await clickWithOptionalPopup(page, /Open Microsoft device login/);
  await expect(page.getByText('2 of 6 complete')).toBeVisible();
  await expect(page.getByText('Bot app identity is ready: bot-app-id-123.')).toBeVisible();

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expectNoRegression(page, '3 of 6 complete');
  await expect(page.getByText('Bot endpoint updated to https://runtime.example.test/v1/messaging/ingress/messaging-teams/demo/default.')).toBeVisible();

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('4 of 6 complete')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Add to Teams' })).toBeVisible();

  await clickWithOptionalPopup(page, /Add to Teams/);
  await expect(page.getByRole('button', { name: 'Verify Teams install' })).toBeVisible();

  await page.getByRole('button', { name: 'Verify Teams install' }).click();
  await expect(page.locator('[data-role="overall"]')).toHaveText('Setup complete');
  await expect(page.getByText('6 of 6 complete')).toBeVisible();

  const requests = await request.get('/requests').then((res) => res.json());
  expect(requests.requests).toEqual(
    expect.arrayContaining([
      expect.objectContaining({
        path: '/api/config',
        body: expect.objectContaining({
          config: expect.objectContaining({
            bot_display_name: 'Custom Greentic Bot',
            public_base_url: 'https://custom-runtime.example.test',
          }),
        }),
      }),
      expect.objectContaining({
        path: '/api/next',
        body: expect.objectContaining({
          config: expect.not.objectContaining({
            oauth_user_code: expect.any(String),
            graph_access_token: expect.any(String),
          }),
        }),
      }),
    ]),
  );

  const events = await page.evaluate(() => (window as unknown as { __setupEvents: Array<{ name: string }> }).__setupEvents);
  expect(events.map((event) => event.name)).toContain('complete');

  const errors = await page.evaluate(async () => {
    return await (window as unknown as { __setupErrors: () => Promise<string[]> }).__setupErrors();
  });
  expect(errors).toEqual([]);
});

test('configuration backend validation errors are shown and save succeeds on retry', async ({ page, request }) => {
  await resetScenario(request, 'config-error');
  await page.goto('/');
  await page.getByText('Advanced configuration').click();

  await page.getByLabel('Public base URL').fill('http://not-https.example.test');
  await page.getByRole('button', { name: 'Save configuration' }).click();
  await expect(page.getByText('Setup error')).toBeVisible();
  await expect(page.getByText('Public base URL is not a valid HTTPS URL')).toBeVisible();

  await page.getByLabel('Public base URL').fill('https://fixed-runtime.example.test');
  await page.getByRole('button', { name: 'Save configuration' }).click();
  await expect(page.getByText('configuration saved')).toBeVisible();
});

test('local preflight blocks Bot Framework registration until required runtime URL is supplied', async ({ page, request }) => {
  await resetScenario(request, 'missing-public-url');
  await startThroughBotIdentity(page, request);

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('Setup needs a public runtime URL before it can register the Teams bot endpoint. Start setup with a public runtime/tunnel URL configured, then refresh.')).toBeVisible();
  await expect(page.getByText('2 of 6 complete')).toBeVisible();

  await page.getByText('Advanced configuration').click();
  await page.getByLabel('Public base URL').fill('https://fixed-runtime.example.test');
  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('3 of 6 complete')).toBeVisible();
});

test('2 of 6 generic pending setup renders Continue setup and posts next', async ({ page, request }) => {
  await resetScenario(request, 'two-complete');
  await page.goto('/');

  await expect(page.getByText('2 of 6 complete')).toBeVisible();
  await expect(page.getByText('click again to continue setup')).toBeVisible();

  const action = await page.locator('greentic-teams-setup-v4').evaluate((node: Element) => {
    return (node as unknown as { _currentAction: () => { kind: string; label: string } })._currentAction();
  });
  expect(action).toEqual(expect.objectContaining({ kind: 'continue', label: 'Continue setup' }));

  await clickContinueAndWaitForNextRequest(page, request);
  const requests = await request.get('/requests').then((res) => res.json());
  expect(requests.requests).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ path: '/api/next' }),
    ]),
  );
  await expect(page.getByText('3 of 6 complete')).toBeVisible();
});

test('HTTP setup action failures show backend diagnostics and retry advances the step', async ({ page, request }) => {
  await resetScenario(request, 'next-http-503');
  await startThroughBotIdentity(page, request);

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('Step did not finish')).toBeVisible();
  await expect(page.getByText('Bot Framework registration service is unavailable')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible();

  await page.getByRole('button', { name: 'Retry' }).click();
  await expect(page.getByText('3 of 6 complete')).toBeVisible();
});

test('provider ok:false setup results are shown and the next retry succeeds', async ({ page, request }) => {
  await resetScenario(request, 'next-transient');
  await startThroughBotIdentity(page, request);

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('Setup error')).toBeVisible();
  await expect(page.locator('[data-role="outcome"] p')).toHaveText('retry Bot Framework registration');
  await expect(page.getByText('2 of 6 complete')).toBeVisible();

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('3 of 6 complete')).toBeVisible();
});

test('Teams app catalog publish errors are visible and retry publishes the app', async ({ page, request }) => {
  await resetScenario(request, 'publish-transient');
  await startThroughBotFrameworkRegistration(page, request);

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('Setup error')).toBeVisible();
  await expect(page.locator('[data-role="outcome"] p')).toHaveText('retry Teams app catalog publish');
  await expect(page.getByText('3 of 6 complete')).toBeVisible();

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.getByText('4 of 6 complete')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Add to Teams' })).toBeVisible();
});

test('device OAuth pending responses keep waiting and then advance without a stale code', async ({ page, request }) => {
  await resetScenario(request, 'oauth-pending-once');
  await page.goto('/');

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.locator('span.code', { hasText: 'GRAPH-CODE-1' })).toBeVisible();
  await clickWithOptionalPopup(page, /Open Microsoft device login/);
  await expect(page.getByText('2 of 6 complete')).toBeVisible();

  const requests = await request.get('/requests').then((res) => res.json());
  const oauthCompleteCalls = requests.requests.filter((entry: { path: string }) => entry.path === '/api/oauth/graph/complete');
  expect(oauthCompleteCalls.length).toBeGreaterThanOrEqual(2);
});

test('expired device codes are refreshed automatically and the OAuth flow recovers', async ({ page, request }) => {
  await resetScenario(request, 'oauth-expired-refresh');
  await page.goto('/');

  await page.getByRole('button', { name: 'Continue setup' }).click();
  await expect(page.locator('span.code', { hasText: 'GRAPH-CODE-1' })).toBeVisible();
  await clickWithOptionalPopup(page, /Open Microsoft device login/);
  await expect(page.getByText('2 of 6 complete')).toBeVisible();

  const requests = await request.get('/requests').then((res) => res.json());
  expect(requests.scenario).toBe('oauth-expired-refresh');
  expect(requests.failures.expiredCode).toBe(true);
  expect(requests.requests).toEqual(
    expect.arrayContaining([
      expect.objectContaining({ path: '/api/oauth/graph/start' }),
    ]),
  );
});

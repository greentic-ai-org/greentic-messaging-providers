import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../../..');
const assetRoot = path.join(repoRoot, 'packs/messaging-webchat-gui/assets/webchat-gui');
const testPagesRoot = path.join(repoRoot, 'tests/webchat-gui/test-pages');

const args = new Map();
for (let index = 2; index < process.argv.length; index += 1) {
  if (process.argv[index].startsWith('--')) {
    args.set(process.argv[index].slice(2), process.argv[index + 1]);
    index += 1;
  }
}
const port = Number(args.get('port') || process.env.WEBCHAT_GUI_TEST_PORT || 8799);

const demoLinks = [
  { id: 'docs', label: 'Docs', url: 'https://docs.greentic.ai' },
  { id: 'playground', label: 'Playground', url: 'https://example.test/playground' },
];

function contentType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  return {
    '.css': 'text/css; charset=utf-8',
    '.html': 'text/html; charset=utf-8',
    '.ico': 'image/x-icon',
    '.jpg': 'image/jpeg',
    '.jpeg': 'image/jpeg',
    '.js': 'text/javascript; charset=utf-8',
    '.json': 'application/json; charset=utf-8',
    '.map': 'application/json; charset=utf-8',
    '.png': 'image/png',
    '.svg': 'image/svg+xml',
  }[ext] || 'application/octet-stream';
}

function safeJoin(root, requestPath) {
  const decoded = decodeURIComponent(requestPath);
  const relative = decoded.replace(/^\/+/, '');
  const resolved = path.resolve(root, relative);
  if (!resolved.startsWith(root)) return null;
  return resolved;
}

function tenantScenario(tenant) {
  const normalized = String(tenant || 'default');
  const skin = normalized.includes('3aigent') ? '3aigent' : 'default';
  return {
    tenant: normalized,
    skin,
    nav: normalized.includes('nav'),
    login: normalized.includes('login'),
  };
}

function tenantConfig(tenant) {
  const scenario = tenantScenario(tenant);
  const basePath = path.join(assetRoot, 'config/tenants/greentic.json');
  const base = JSON.parse(fs.readFileSync(basePath, 'utf8'));
  base.tenant_id = scenario.tenant;
  base.legacy_skin = scenario.skin;
  base.skin = scenario.skin;
  base.branding = {
    company_name: scenario.skin === '3aigent' ? '3AIgent' : 'Greentic',
    tagline: scenario.skin === '3aigent' ? 'Deep Research' : 'Greentic AI Assistant',
    logo: scenario.skin === '3aigent'
      ? '/skins/3aigent/assets/3point-jewel-round.png'
      : '/skins/default/assets/logo.svg',
  };
  base.navigation = scenario.nav ? { menu: demoLinks } : { menu: [] };
  base.nav_links = scenario.nav ? demoLinks : [];
  delete base.auth;
  return base;
}

function send(res, status, headers, body) {
  res.writeHead(status, {
    'Cache-Control': 'no-store',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Headers': 'authorization,content-type,x-greentic-locale',
    'Access-Control-Allow-Methods': 'GET,POST,OPTIONS',
    ...headers,
  });
  res.end(body);
}

function sendJson(res, status, value) {
  send(res, status, { 'Content-Type': 'application/json; charset=utf-8' }, JSON.stringify(value));
}

function serveFile(res, filePath) {
  fs.stat(filePath, (statError, stat) => {
    if (statError || !stat.isFile()) {
      sendJson(res, 404, { error: 'not found' });
      return;
    }
    send(res, 200, { 'Content-Type': contentType(filePath) }, fs.readFileSync(filePath));
  });
}

function appAssetPath(urlPath) {
  const match = urlPath.match(/^\/v1\/web\/webchat\/[^/]+\/?(.*)$/);
  if (!match) return null;
  const rest = match[1] || 'index.html';
  return safeJoin(assetRoot, rest || 'index.html');
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url || '/', `http://${req.headers.host || '127.0.0.1'}`);
  const urlPath = url.pathname;

  if (req.method === 'OPTIONS') {
    send(res, 204, {}, '');
    return;
  }
  if (urlPath === '/healthz') {
    sendJson(res, 200, { ok: true });
    return;
  }
  if (urlPath === '/favicon.ico') {
    serveFile(res, path.join(assetRoot, 'skins/default/assets/favicon.ico'));
    return;
  }
  if (urlPath === '/') {
    send(res, 302, { Location: '/host.html' }, '');
    return;
  }
  if (urlPath === '/mock-api/messages' && req.method === 'POST') {
    req.resume();
    sendJson(res, 200, { text: 'Hello from Greentic' });
    return;
  }
  if (urlPath.endsWith('/auth/config')) {
    const tenant = urlPath.split('/v1/messaging/webchat/')[1]?.split('/')[0] || 'default';
    const scenario = tenantScenario(tenant);
    sendJson(res, 200, scenario.login
      ? { enabled: true, providers: [{ id: 'test-login', label: 'Test Login', type: 'dummy', enabled: true }] }
      : { enabled: false });
    return;
  }
  if (urlPath.endsWith('/token') || urlPath.endsWith('/v3/directline/tokens/generate')) {
    req.resume();
    sendJson(res, 200, {
      conversationId: 'webchat-gui-test',
      token: 'webchat-gui-test-token',
      expires_in: 1800,
    });
    return;
  }
  if (urlPath.includes('/v3/directline/conversations')) {
    req.resume();
    sendJson(res, 200, { activities: [], watermark: '0', id: 'mock-activity' });
    return;
  }
  const tenantConfigMatch = urlPath.match(/^\/v1\/web\/webchat\/([^/]+)\/config\/tenants\/([^/]+)\.json$/);
  if (tenantConfigMatch) {
    if (tenantConfigMatch[2].includes('missing-config')) {
      sendJson(res, 404, { error: 'tenant config not found' });
      return;
    }
    sendJson(res, 200, tenantConfig(tenantConfigMatch[2]));
    return;
  }
  if (/^\/v1\/web\/webchat\/[^/]+\/i18n\/_manifest\.json$/.test(urlPath)) {
    sendJson(res, 200, { locales: ['en'] });
    return;
  }
  if (/^\/v1\/web\/webchat\/[^/]+\/i18n\/en-US\.json$/.test(urlPath)) {
    serveFile(res, path.join(assetRoot, 'i18n/en.json'));
    return;
  }
  if (urlPath.startsWith('/skins/')) {
    const filePath = safeJoin(assetRoot, urlPath);
    if (filePath) serveFile(res, filePath);
    else sendJson(res, 403, { error: 'forbidden' });
    return;
  }
  if (urlPath.startsWith('/test-pages/')) {
    const filePath = safeJoin(testPagesRoot, urlPath.replace(/^\/test-pages\//, ''));
    if (filePath) serveFile(res, filePath);
    else sendJson(res, 403, { error: 'forbidden' });
    return;
  }
  const appPath = appAssetPath(urlPath);
  if (appPath) {
    serveFile(res, fs.existsSync(appPath) && fs.statSync(appPath).isDirectory() ? path.join(appPath, 'index.html') : appPath);
    return;
  }
  if (urlPath.endsWith('.map')) {
    send(res, 204, { 'Content-Type': 'application/json; charset=utf-8' }, '');
    return;
  }
  sendJson(res, 404, { error: 'not found', path: urlPath });
});

server.listen(port, '127.0.0.1', () => {
  console.log(`WebChat GUI Playwright server listening on http://127.0.0.1:${port}`);
});

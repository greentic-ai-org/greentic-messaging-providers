import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, '../../..');
const componentPath = path.join(root, 'messaging-teams/assets/setup/greentic-teams-setup.js');
const portArg = process.argv.indexOf('--port');
const port = portArg >= 0 ? Number(process.argv[portArg + 1]) : 8811;

const stepIds = [
  'graph_admin_consent',
  'bot_app_identity',
  'bot_framework_endpoint_registration',
  'teams_app_publish',
  'teams_app_user_install',
  'first_bot_framework_post',
];

const stepLabels = {
  graph_admin_consent: 'graph admin consent',
  bot_app_identity: 'bot app identity',
  bot_framework_endpoint_registration: 'bot framework endpoint registration',
  teams_app_publish: 'teams app publish',
  teams_app_user_install: 'teams app user install',
  first_bot_framework_post: 'first bot framework post',
};

function initialState() {
  return {
    scenario: 'happy',
    done: new Set(),
    staleSnapshots: [],
    oauthCompletePolls: 0,
    failures: {},
    requests: [],
    values: {
      backend: null,
      config: {
        tenant: 'demo',
        team: 'default',
        env: 'dev',
        bot_display_name: 'Greentic Bot',
        public_base_url: 'https://runtime.example.test',
        bot_framework_registration_url: 'https://runtime.example.test/v1/setup/bot-framework/registration',
      },
      last_setup_result: null,
    },
  };
}

let state = initialState();

function resetState(scenario = 'happy') {
  state = initialState();
  state.scenario = scenario;
  if (scenario === 'missing-public-url') {
    state.values.config.public_base_url = '';
  }
  if (scenario === 'two-complete') {
    state.done.add('graph_admin_consent');
    state.done.add('bot_app_identity');
    state.values.oauth = {
      graph: { ok: true, token_store_key: 'graph_access_token', completed_at: Date.now() },
    };
    state.values.config.bot_app_id = 'bot-app-id-123';
    state.values.config.bot_app_password = 'bot-secret-xyz';
    setResult('bot_app_identity', true, {
      ok: true,
      action: 'keep',
      app_id: 'bot-app-id-123',
      bot_app_id: 'bot-app-id-123',
    }, 'click again to continue setup');
  }
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function doneCountFromSnapshot(snapshot) {
  return snapshot.setup_status.items.filter((item) => item.state === 'done').length;
}

function status(next = 'Click Continue setup to start.') {
  const items = stepIds.map((id) => ({
    id,
    label: stepLabels[id],
    state: state.done.has(id) ? 'done' : 'pending',
    detail: null,
  }));
  return {
    ok: items.every((item) => item.state === 'done'),
    blocked: null,
    items,
    last_step: state.values.last_setup_result?.step || null,
    next,
    selected: {
      env: 'dev',
      provider_id: 'messaging-teams',
      tenant: 'demo',
      team: 'default',
    },
  };
}

function publicState(next) {
  const nextText = next || (state.scenario === 'two-complete'
    ? 'click again to continue setup'
    : undefined);
  return {
    ok: true,
    setup_status: status(nextText),
    teams_app: {
      ok: state.done.has('teams_app_publish'),
      add_to_teams_url: state.done.has('teams_app_publish') ? '/fake-teams/add' : null,
      open_bot_chat_url: state.done.has('teams_app_user_install') ? '/fake-teams/chat' : null,
    },
    values: clone(state.values),
  };
}

function setResult(step, ok, result, next) {
  state.values.last_setup_result = { step, ok, result, next };
}

function complete(step, result = {}, next = 'Click Continue setup to continue.') {
  state.done.add(step);
  setResult(step, true, result, next);
}

function responseJson(res, statusCode, body) {
  const payload = JSON.stringify(body);
  res.writeHead(statusCode, {
    'content-type': 'application/json; charset=utf-8',
    'content-length': Buffer.byteLength(payload),
  });
  res.end(payload);
}

function responseText(res, statusCode, body, contentType = 'text/plain; charset=utf-8') {
  res.writeHead(statusCode, {
    'content-type': contentType,
    'content-length': Buffer.byteLength(body),
  });
  res.end(body);
}

async function readJson(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  if (chunks.length === 0) return {};
  return JSON.parse(Buffer.concat(chunks).toString('utf8'));
}

function currentDone() {
  return stepIds.filter((id) => state.done.has(id)).length;
}

function startGraphLogin(userCode = 'GRAPH-CODE-1') {
  state.values.config.oauth_kind = 'graph';
  state.values.config.oauth_user_code = userCode;
  state.values.config.oauth_verification_uri = 'https://login.microsoft.test/device';
  state.values.last_oauth = {
    kind: 'graph',
    response: {
      user_code: userCode,
      verification_uri: 'https://login.microsoft.test/device',
      expires_in: 900,
      interval: 1,
      message: `Enter ${userCode}`,
    },
  };
  setResult('graph_admin_consent', false, {
    ok: false,
    pending_device_login: true,
    login: {
      user_code: userCode,
      userCode: userCode,
      url: 'https://login.microsoft.test/device',
      expiresIn: 900,
      interval: 1,
    },
    body: state.values.last_oauth.response,
  }, 'authorize in the opened browser, then wait for setup to continue');
  return publicState('authorize in the opened browser, then wait for setup to continue');
}

function nextStep() {
  const done = currentDone();
  if (done === 0 && !state.values.oauth?.graph?.ok) return startGraphLogin();
  if (done === 1) {
    state.values.config.bot_app_id = 'bot-app-id-123';
    state.values.config.bot_app_password = 'bot-secret-xyz';
    complete('bot_app_identity', {
      ok: true,
      action: 'create',
      app_id: 'bot-app-id-123',
      bot_app_id: 'bot-app-id-123',
      secret_action: 'generated_secret',
    });
    return publicState('click again to continue setup');
  }
  if (done === 2) {
    if (state.scenario === 'next-http-503' && !state.failures.botFrameworkHttp) {
      state.failures.botFrameworkHttp = true;
      return {
        httpStatus: 503,
        ok: false,
        error: 'Bot Framework registration service is unavailable',
      };
    }
    if (state.scenario === 'next-transient' && !state.failures.botFrameworkRegistration) {
      state.failures.botFrameworkRegistration = true;
      return {
        ok: false,
        error: 'Bot Framework registration temporarily unavailable',
        step: 'bot_framework_endpoint_registration',
        next: 'retry Bot Framework registration',
      };
    }
    const before = publicState('click again to continue setup');
    complete('bot_framework_endpoint_registration', {
      ok: true,
      action: 'update',
      target_messaging_endpoint: 'https://runtime.example.test/v1/messaging/ingress/messaging-teams/demo/default',
      current_messaging_endpoint: 'https://runtime.example.test/v1/messaging/ingress/messaging-teams/demo/default',
      teams_channel: { ok: true },
    });
    state.staleSnapshots.push(before, before);
    return publicState('click again to continue setup');
  }
  if (done === 3) {
    if (state.scenario === 'publish-transient' && !state.failures.publish) {
      state.failures.publish = true;
      return {
        ok: false,
        error: 'Teams app catalog publish failed.',
        step: 'teams_app_publish',
        next: 'retry Teams app catalog publish',
      };
    }
    complete('teams_app_publish', {
      ok: true,
      add_to_teams_url: '/fake-teams/add',
      catalog_app_id: 'teams-catalog-id-123',
    }, 'open the Add to Teams link, install the app, then continue');
    state.values.last_teams_app_publish = state.values.last_setup_result.result;
    return publicState('open the Add to Teams link, install the app, then continue');
  }
  if (done === 4) {
    complete('teams_app_user_install', {
      ok: true,
      action: 'install',
      open_bot_chat_url: '/fake-teams/chat',
      installed_app_id: 'installed-app-id-123',
    }, 'open the bot chat and send hello');
    state.values.last_teams_app_install = state.values.last_setup_result.result;
    return publicState('open the bot chat and send hello');
  }
  if (done === 5) {
    state.values.last_activity = {
      serviceUrl: 'https://smba.trafficmanager.net/emea/',
      conversation: { id: 'conversation-id-123' },
      text: 'hello',
    };
    complete('first_bot_framework_post', {
      ok: true,
      serviceUrl: 'https://smba.trafficmanager.net/emea/',
      conversation: { id: 'conversation-id-123' },
    }, 'setup complete');
    return publicState('setup complete');
  }
  return publicState('setup complete');
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url || '/', `http://${req.headers.host}`);
  try {
    if (url.pathname === '/healthz') return responseText(res, 200, 'ok');
    if (url.pathname === '/reset') {
      resetState(url.searchParams.get('scenario') || 'happy');
      return responseJson(res, 200, { ok: true });
    }
    if (url.pathname === '/requests') {
      return responseJson(res, 200, {
        scenario: state.scenario,
        failures: state.failures,
        oauthCompletePolls: state.oauthCompletePolls,
        requests: state.requests,
      });
    }
    if (url.pathname === '/component.js') {
      return responseText(res, 200, fs.readFileSync(componentPath, 'utf8'), 'application/javascript; charset=utf-8');
    }
    if (url.pathname === '/') {
      return responseText(res, 200, `<!doctype html>
        <html lang="en">
          <head><meta charset="utf-8"><title>Teams setup fixture</title></head>
          <body>
            <greentic-teams-setup-v4
              advanced="true"
              poll-interval="500"
              action-timeout="12000"
              provider-id="messaging-teams"
              state-path="/api/state"
              next-path="/api/next"
              config-path="/api/config"
              oauth-start-path="/api/oauth/{kind}/start"
              oauth-complete-path="/api/oauth/{kind}/complete"
              package-path="/package.zip"></greentic-teams-setup-v4>
            <script type="module" src="/component.js"></script>
            <script>
              window.__setupEvents = [];
              for (const name of ['state', 'result', 'action-start', 'action-complete', 'action-timeout', 'request-start', 'request-success', 'request-error', 'skip-next', 'complete']) {
                window.addEventListener('greentic-provider-setup-' + name, (event) => {
                  window.__setupEvents.push({ name, detail: event.detail });
                });
              }
            </script>
          </body>
        </html>`, 'text/html; charset=utf-8');
    }
    if (url.pathname === '/api/state' && req.method === 'GET') {
      if (state.done.has('teams_app_user_install') && !state.done.has('first_bot_framework_post')) {
        state.values.last_activity = {
          serviceUrl: 'https://smba.trafficmanager.net/emea/',
          conversation: { id: 'conversation-id-123' },
          text: 'hello',
        };
        complete('first_bot_framework_post', {
          ok: true,
          serviceUrl: 'https://smba.trafficmanager.net/emea/',
          conversation: { id: 'conversation-id-123' },
        }, 'setup complete');
      }
      const snapshot = state.staleSnapshots.shift() || publicState();
      return responseJson(res, 200, snapshot);
    }
    if (url.pathname === '/api/config' && req.method === 'POST') {
      const body = await readJson(req);
      state.requests.push({ path: url.pathname, body });
      if (state.scenario === 'config-error' && !state.failures.config) {
        state.failures.config = true;
        return responseJson(res, 400, {
          ok: false,
          error: 'Public base URL is not a valid HTTPS URL',
          field: 'public_base_url',
        });
      }
      state.values.config = { ...state.values.config, ...(body.config || {}) };
      return responseJson(res, 200, publicState('configuration saved'));
    }
    if (url.pathname === '/api/next' && req.method === 'POST') {
      const body = await readJson(req);
      state.requests.push({ path: url.pathname, body });
      state.values.config = { ...state.values.config, ...(body.config || {}) };
      const result = nextStep();
      return responseJson(res, result.httpStatus || 200, result);
    }
    if (url.pathname === '/api/oauth/graph/start' && req.method === 'POST') {
      const body = await readJson(req);
      state.requests.push({ path: url.pathname, body });
      return responseJson(res, 200, startGraphLogin('GRAPH-CODE-2'));
    }
    if (url.pathname === '/api/oauth/graph/complete' && req.method === 'POST') {
      const body = await readJson(req);
      state.requests.push({ path: url.pathname, body });
      state.oauthCompletePolls += 1;
      if (state.scenario === 'oauth-pending-once' && state.oauthCompletePolls === 1) {
        return responseJson(res, 200, {
          ok: false,
          result: {
            body: {
              error: 'authorization_pending',
              error_description: 'Authorization is pending.',
              error_codes: [70016],
            },
          },
        });
      }
      if (state.scenario === 'oauth-expired-refresh' && !state.failures.expiredCode) {
        state.failures.expiredCode = true;
        return responseJson(res, 200, {
          ok: false,
          result: {
            body: {
              error: 'expired_token',
              error_description: 'The device code expired.',
              error_codes: [70020],
            },
          },
        });
      }
      state.done.add('graph_admin_consent');
      state.values.oauth = {
        ...(state.values.oauth || {}),
        graph: { ok: true, token_store_key: 'graph_access_token', completed_at: Date.now() },
      };
      complete('graph_admin_consent', { ok: true }, 'click again to continue setup');
      return responseJson(res, 200, { ok: true, step: 'graph_admin_consent' });
    }
    if (url.pathname === '/fake-teams/add' || url.pathname === '/fake-teams/chat') {
      return responseText(res, 200, 'fake teams');
    }
    return responseJson(res, 404, { ok: false, error: 'not found', path: url.pathname });
  } catch (error) {
    return responseJson(res, 500, { ok: false, error: error.message, stack: error.stack });
  }
});

server.listen(port, '127.0.0.1');

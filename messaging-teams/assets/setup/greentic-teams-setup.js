const template = document.createElement("template");

template.innerHTML = `
  <style>
    :host {
      --gts-accent: #0f766e;
      --gts-accent-hover: #115e59;
      --gts-accent-soft: #ccfbf1;
      --gts-danger: #b91c1c;
      --gts-danger-soft: #fee2e2;
      --gts-warning: #b45309;
      --gts-warning-soft: #fef3c7;
      --gts-success: #047857;
      --gts-success-soft: #d1fae5;
      --gts-border: #d8dee4;
      --gts-muted: #64748b;
      --gts-panel: #ffffff;
      --gts-page: #f8fafc;
      --gts-text: #0f172a;
      --gts-radius: 8px;
      --gts-focus: rgba(15, 118, 110, 0.28);
      display: block;
      color: var(--gts-text);
      font: 14px/1.45 Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    * {
      box-sizing: border-box;
    }

    h2,
    h3,
    p {
      margin: 0;
    }

    .shell {
      display: grid;
      gap: 14px;
      padding: 16px;
      border: 1px solid var(--gts-border);
      border-radius: var(--gts-radius);
      background: var(--gts-panel);
    }

    .top {
      display: grid;
      gap: 10px;
    }

    .title-row {
      display: flex;
      justify-content: space-between;
      gap: 12px;
      align-items: start;
    }

    h2 {
      font-size: 18px;
      font-weight: 650;
      letter-spacing: 0;
    }

    h3 {
      font-size: 14px;
      font-weight: 650;
      letter-spacing: 0;
    }

    .muted {
      color: var(--gts-muted);
    }

    .progress {
      display: grid;
      gap: 7px;
    }

    .bar {
      height: 8px;
      overflow: hidden;
      border-radius: 999px;
      background: #e2e8f0;
    }

    .bar span {
      display: block;
      height: 100%;
      width: var(--progress, 0%);
      border-radius: inherit;
      background: var(--gts-accent);
      transition: width 180ms ease;
    }

    .action {
      display: grid;
      gap: 12px;
      padding: 14px;
      border-radius: var(--gts-radius);
      border: 1px solid var(--gts-border);
      background: var(--gts-page);
    }

    .buttons {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }

    button,
    a.button {
      min-height: 38px;
      border: 1px solid var(--gts-border);
      border-radius: 6px;
      padding: 0 13px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 8px;
      color: var(--gts-text);
      background: #fff;
      text-decoration: none;
      cursor: pointer;
      font: inherit;
      white-space: nowrap;
    }

    button.primary,
    a.button.primary {
      border-color: var(--gts-accent);
      color: #fff;
      background: var(--gts-accent);
    }

    button.primary:hover,
    a.button.primary:hover {
      background: var(--gts-accent-hover);
    }

    button:disabled {
      opacity: 0.58;
      cursor: not-allowed;
    }

    button:focus-visible,
    a.button:focus-visible,
    input:focus-visible {
      outline: 3px solid var(--gts-focus);
      outline-offset: 2px;
    }

    .pill {
      min-height: 26px;
      border-radius: 999px;
      padding: 3px 9px;
      display: inline-flex;
      align-items: center;
      width: max-content;
      color: var(--gts-muted);
      background: #e2e8f0;
      font-size: 12px;
      font-weight: 650;
    }

    .pill.done {
      color: var(--gts-success);
      background: var(--gts-success-soft);
    }

    .pill.blocked {
      color: var(--gts-danger);
      background: var(--gts-danger-soft);
    }

    .pill.pending {
      color: var(--gts-warning);
      background: var(--gts-warning-soft);
    }

    .callout {
      display: grid;
      gap: 10px;
      padding: 12px;
      border-radius: 6px;
      border: 1px solid var(--gts-border);
      background: #fff;
    }

    .callout.oauth {
      border-color: var(--gts-accent);
      background: var(--gts-accent-soft);
    }

    .callout.blocked,
    .callout.error {
      border-color: var(--gts-danger);
      background: var(--gts-danger-soft);
    }

    .callout.running {
      border-color: var(--gts-warning);
      background: var(--gts-warning-soft);
    }

    .code {
      display: inline-flex;
      align-items: center;
      max-width: 100%;
      min-height: 31px;
      padding: 3px 8px;
      border: 1px solid var(--gts-border);
      border-radius: 6px;
      background: rgba(255, 255, 255, 0.78);
      font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      overflow-wrap: anywhere;
    }

    .steps {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }

    details {
      border-top: 1px solid var(--gts-border);
      padding-top: 10px;
    }

    summary {
      cursor: pointer;
      color: var(--gts-muted);
      font-weight: 650;
    }

    .fields {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;
      margin-top: 12px;
    }

    label {
      display: grid;
      gap: 4px;
      color: var(--gts-muted);
      font-size: 12px;
    }

    input {
      width: 100%;
      min-height: 34px;
      border: 1px solid var(--gts-border);
      border-radius: 6px;
      padding: 0 9px;
      color: var(--gts-text);
      background: #fff;
      font: inherit;
    }

    .wide {
      grid-column: 1 / -1;
    }

    pre {
      max-height: 220px;
      overflow: auto;
      margin: 10px 0 0;
      padding: 10px;
      border-radius: 6px;
      background: #0f172a;
      color: #e2e8f0;
      font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      white-space: pre-wrap;
    }

    [hidden] {
      display: none !important;
    }

    @media (max-width: 720px) {
      .title-row {
        display: grid;
      }

      .fields {
        grid-template-columns: 1fr;
      }
    }
  </style>

  <section class="shell" part="shell">
    <div class="top">
      <div class="title-row">
        <div>
          <h2 data-i18n="title"></h2>
          <p class="muted" data-i18n="subtitle"></p>
        </div>
        <span class="pill pending" data-role="overall"></span>
      </div>
      <div class="progress">
        <div class="bar" aria-hidden="true"><span data-role="progress"></span></div>
        <p class="muted" data-role="progressText"></p>
      </div>
    </div>

    <section class="callout" data-role="outcome" hidden></section>

    <section class="action">
      <h3 data-i18n="nextAction"></h3>
      <p data-role="next"></p>
      <div class="buttons" data-role="actions"></div>
    </section>

    <section class="callout running" data-role="runStatus" hidden></section>
    <section class="callout oauth" data-role="oauth" hidden></section>
    <section class="callout blocked" data-role="blocked" hidden></section>
    <section class="callout error" data-role="error" hidden></section>

    <details data-role="stepsDetails">
      <summary data-i18n="showSteps"></summary>
      <div class="steps" data-role="steps"></div>
    </details>

    <details data-role="advancedDetails" hidden>
      <summary data-i18n="advanced"></summary>
      <div class="fields" data-role="fields"></div>
      <div class="buttons wide">
        <button type="button" class="primary" data-action="save" data-i18n="save"></button>
      </div>
      <pre data-role="result"></pre>
    </details>
  </section>
`;

const DEFAULT_TRANSLATIONS = {
  en: {
    title: "Teams setup",
    subtitle: "Follow the next action to publish and test the Greentic Teams bot.",
    continue: "Continue setup",
    refresh: "Refresh",
    refreshAfterManualAction: "Refresh after admin action",
    working: "Working...",
    waitingForNextStep: "Waiting for the setup to move to the next step.",
    refreshingCode: "Refreshing code...",
    stepTimedOutTitle: "Step did not finish",
    stepTimedOut: "The expected next step did not appear before the timeout. Retry the action or use advanced diagnostics.",
    missingPublicBaseUrl: "Setup needs a public runtime URL before it can register the Teams bot endpoint. Start setup with a public runtime/tunnel URL configured, then refresh.",
    missingRegistrationService: "Setup host has not provided a Greentic Bot Service registration endpoint yet. This is a setup/runtime integration issue, not an admin field.",
    retry: "Retry",
    refreshCode: "Refresh code",
    nextAction: "Next action",
    completedTitle: "Completed",
    showSteps: "Show checklist",
    advanced: "Advanced configuration",
    save: "Save configuration",
    loading: "Loading setup status...",
    ready: "Ready",
    inProgress: "In progress",
    complete: "Setup complete",
    blocked: "Blocked",
    progress: "{done} of {total} complete",
    noNext: "Click Continue setup to start.",
    addToTeams: "Add to Teams",
    verifyTeamsInstall: "Verify Teams install",
    openBotChat: "Open bot chat",
    downloadPackage: "Download app package",
    openDeviceLogin: "Open Microsoft device login",
    copyCode: "Copy code",
    codeCopied: "Code copied.",
    oauthTitle: "Microsoft sign-in required",
    oauthInstruction: "Open the sign-in page and enter this code. The setup will continue after authorization.",
    manualTitle: "Admin action required",
    errorTitle: "Setup error",
    actionFailed: "The setup action failed.",
    action: "Action",
    role: "Role",
    scope: "Scope",
    outcomes: {
      start_graph_login: "Microsoft Graph sign-in started.",
      wait_for_graph_login: "Waiting for Microsoft Graph sign-in to finish.",
      complete_graph_login: "Microsoft Graph sign-in completed.",
      create_or_reuse_bot_app: "Bot app identity is ready.",
      reconcile_bot_framework_registration: "Greentic Bot Service registration was updated.",
      greentic_bot_service_ready: "Greentic Bot Service is ready.",
      publish_teams_app: "Teams app was published to the tenant catalog.",
      install_teams_app_for_user: "Teams app was installed for this user.",
      reconcile_azure_bot_resource: "Bot Framework endpoint was reconciled.",
      start_azure_management_login: "Azure management sign-in started.",
      discover_azure_defaults: "Azure target settings were discovered."
    },
    fields: {
      tenant: "Tenant",
      team: "Team",
      runtime_provider: "Runtime provider",
      teams_app_version: "Teams app version",
      teams_app_id: "Teams app ID",
      bot_display_name: "Bot display name",
      public_base_url: "Public base URL",
      bot_framework_registration_url: "Bot Framework registration URL",
      bot_app_id: "Microsoft Bot app ID",
      bot_app_password: "Microsoft Bot app password",
      azure_auth_tenant: "Azure auth tenant",
      graph_setup_client_id: "Graph setup client ID",
      azure_setup_client_id: "Azure setup client ID",
      azure_subscription_id: "Azure subscription ID",
      azure_resource_group: "Azure resource group",
      azure_resource_group_location: "Resource group location",
      azure_bot_name: "Azure Bot name",
      azure_location: "Azure location"
    }
  },
  nl: {
    title: "Teams setup",
    subtitle: "Volg de volgende actie om de Greentic Teams-bot te publiceren en testen.",
    continue: "Setup vervolgen",
    refresh: "Vernieuwen",
    refreshAfterManualAction: "Vernieuwen na beheeractie",
    working: "Bezig...",
    waitingForNextStep: "Wachten tot de setup naar de volgende stap gaat.",
    refreshingCode: "Code vernieuwen...",
    stepTimedOutTitle: "Stap niet voltooid",
    stepTimedOut: "De verwachte volgende stap verscheen niet binnen de tijd. Probeer opnieuw of gebruik geavanceerde diagnostiek.",
    missingPublicBaseUrl: "Setup heeft een publieke runtime-URL nodig voordat de Teams-botendpoint geregistreerd kan worden. Start setup met een publieke runtime/tunnel-URL en vernieuw.",
    missingRegistrationService: "De setup-host heeft nog geen Greentic Bot Service registratie-endpoint geleverd. Dit is een setup/runtime-integratieprobleem, geen beheerderveld.",
    retry: "Opnieuw proberen",
    refreshCode: "Code vernieuwen",
    nextAction: "Volgende actie",
    completedTitle: "Voltooid",
    showSteps: "Checklist tonen",
    advanced: "Geavanceerde configuratie",
    save: "Configuratie opslaan",
    loading: "Setupstatus laden...",
    ready: "Gereed",
    inProgress: "Bezig",
    complete: "Setup compleet",
    blocked: "Geblokkeerd",
    progress: "{done} van {total} gereed",
    noNext: "Klik op Setup vervolgen om te starten.",
    addToTeams: "Toevoegen aan Teams",
    verifyTeamsInstall: "Teams-installatie controleren",
    openBotChat: "Botchat openen",
    downloadPackage: "App-pakket downloaden",
    openDeviceLogin: "Microsoft sign-in openen",
    copyCode: "Code kopieren",
    codeCopied: "Code gekopieerd.",
    oauthTitle: "Microsoft sign-in vereist",
    oauthInstruction: "Open de sign-in pagina en voer deze code in. De setup gaat verder na autorisatie.",
    manualTitle: "Beheeractie vereist",
    errorTitle: "Setupfout",
    actionFailed: "De setupactie is mislukt.",
    action: "Actie",
    role: "Rol",
    scope: "Scope",
    outcomes: {},
    fields: {}
  }
};

const CONFIG_FIELDS = [
  "tenant",
  "team",
  "runtime_provider",
  "teams_app_version",
  "teams_app_id",
  "bot_display_name",
  "public_base_url",
  "bot_app_id",
  "bot_app_password",
  "azure_auth_tenant",
  "graph_setup_client_id",
  "azure_setup_client_id",
  "azure_subscription_id",
  "azure_resource_group",
  "azure_resource_group_location",
  "azure_bot_name",
  "azure_location"
];

function mergeDeep(base, override) {
  if (!override || typeof override !== "object") return base;
  const output = { ...base };
  for (const [key, value] of Object.entries(override)) {
    if (value && typeof value === "object" && !Array.isArray(value)) {
      output[key] = mergeDeep(base[key] || {}, value);
    } else {
      output[key] = value;
    }
  }
  return output;
}

function boolAttr(value, fallback = false) {
  if (value == null) return fallback;
  return value === "" || value === "true" || value === "1";
}

function safeJson(value) {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function findDeviceLogin(value) {
  if (!value || typeof value !== "object") return null;
  const url = value.verification_uri || value.verification_url;
  if (url) {
    return {
      url,
      userCode: value.user_code || value.userCode || "",
      message: value.message || "",
      interval: Number(value.interval || 5),
      expiresIn: Number(value.expires_in || value.expiresIn || 900)
    };
  }
  return findDeviceLogin(value.body) || findDeviceLogin(value.result);
}

function authorizationPending(value) {
  const body = (value && value.result && value.result.body) || (value && value.body) || {};
  return body.error === "authorization_pending" || body.error === "slow_down";
}

function deviceCodeInvalid(value) {
  const body = (value && value.result && value.result.body) || (value && value.body) || {};
  const codes = Array.isArray(body.error_codes) ? body.error_codes : [];
  return body.error === "expired_token"
    || body.error === "authorization_declined"
    || body.error === "invalid_grant"
    || codes.includes(70020)
    || codes.includes(7000014);
}

function pendingDeviceLogin(value) {
  return String(value && value.step || "").startsWith("wait_for_") && authorizationPending(value);
}

function href(base, path) {
  const cleanBase = String(base || "").replace(/\/+$/, "");
  return `${cleanBase}${path}`;
}

export class GreenticTeamsSetup extends HTMLElement {
  static get observedAttributes() {
    return [
      "api-base",
      "locale",
      "poll-interval",
      "auto-poll",
      "advanced",
      "action-timeout",
      "provider-id",
      "state-path",
      "next-path",
      "config-path",
      "oauth-start-path",
      "oauth-complete-path",
      "package-path"
    ];
  }

  static translations = DEFAULT_TRANSLATIONS;

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.shadowRoot.append(template.content.cloneNode(true));
    this._state = null;
    this._lastResult = null;
    this._translations = null;
    this._pollTimer = 0;
    this._runState = null;
    this._deviceLoginRunning = false;
    this._lastManagedAction = null;
    this._copyMessage = "";
    this._codeRefreshBusy = false;
    this._lastCompleteSignature = "";
    this._draftConfig = {};
    this._manualActions = {};
    this._boundClick = this._onClick.bind(this);
    this._boundInput = this._onInput.bind(this);
  }

  connectedCallback() {
    this._manualActions = this._readManualActions();
    this.shadowRoot.addEventListener("click", this._boundClick);
    this.shadowRoot.addEventListener("input", this._boundInput);
    this._renderStaticText();
    this._renderFields();
    this.refresh();
    this._configurePolling();
  }

  disconnectedCallback() {
    this.shadowRoot.removeEventListener("click", this._boundClick);
    this.shadowRoot.removeEventListener("input", this._boundInput);
    this._stopPolling();
  }

  attributeChangedCallback() {
    if (!this.isConnected) return;
    this._render();
    this._configurePolling();
  }

  set translations(value) {
    this._translations = value;
    this._render();
  }

  get translations() {
    return this._translations;
  }

  get apiBase() {
    return this.getAttribute("api-base") || "";
  }

  _endpoint(name, params = {}) {
    const defaults = {
      state: "/api/state",
      next: "/api/setup/next",
      config: "/api/config",
      oauthStart: "/api/oauth/{kind}/start",
      oauthComplete: "/api/oauth/{kind}/complete",
      package: "/teams-app/package.zip"
    };
    const attr = {
      state: "state-path",
      next: "next-path",
      config: "config-path",
      oauthStart: "oauth-start-path",
      oauthComplete: "oauth-complete-path",
      package: "package-path"
    }[name];
    let path = (attr && this.getAttribute(attr)) || defaults[name];
    for (const [key, value] of Object.entries(params)) {
      path = path.replaceAll(`{${key}}`, encodeURIComponent(String(value)));
    }
    return path;
  }

  get locale() {
    return this.getAttribute("locale") || document.documentElement.lang || "en";
  }

  get pollInterval() {
    return Math.max(1000, Number(this.getAttribute("poll-interval") || 3000));
  }

  get autoPoll() {
    return boolAttr(this.getAttribute("auto-poll"), true);
  }

  get advanced() {
    return this.hasAttribute("advanced") && this.getAttribute("advanced") !== "false";
  }

  get actionTimeout() {
    return Math.max(5000, Number(this.getAttribute("action-timeout") || 120000));
  }

  get providerId() {
    return this.getAttribute("provider-id") || "messaging-teams";
  }

  async refresh() {
    this._setBusy(true);
    try {
      const state = await this._request("GET", this._endpoint("state"));
      this._applyState(state);
      const values = state && state.values || {};
      if ((values.last_teams_app_install || {}).ok && this._manualActions.addToTeamsOpened) {
        this._manualActions.addToTeamsOpened = false;
        this._writeManualActions();
      }
      if (this._runState?.state === "timeout" && this._advanced(this._runState.before, this._snapshot(), this._runState.action)) {
        this._runState = null;
      }
      this._error(null);
      this._emit("state", { state: this._state });
      this._emitCompleteIfDone(state);
      this._render();
    } catch (error) {
      this._error(error);
    } finally {
      this._setBusy(false);
    }
  }

  async runNextStep() {
    return this._postAndRefresh(this._endpoint("next"), this._collectConfig(), true);
  }

  async saveConfiguration() {
    return this._postAndRefresh(this._endpoint("config"), this._collectConfig(), false);
  }

  async publishTeamsApp() {
    return this.runNextStep();
  }

  async installTeamsApp() {
    return this.runNextStep();
  }

  async runCurrentAction() {
    const action = this._currentAction();
    if (!action) {
      this._emitSkipNext("no-current-action", action);
      return null;
    }
    if (this._runState?.state === "running") {
      this._emitSkipNext("action-already-running", action);
      return null;
    }
    const preflight = this._preflightAction(action);
    if (preflight) {
      this.setAttribute("advanced", "true");
      this._localError(preflight);
      this._render();
      this._emitSkipNext("preflight-blocked", action, { message: preflight });
      return null;
    }
    this._lastManagedAction = action;
    return this._executeManagedAction(action);
  }

  async retryCurrentAction() {
    let action = this._lastManagedAction || this._currentAction();
    if (!action || this._runState?.state === "running") return null;
    if (this._staleManagedAction(action)) {
      action = this._currentAction();
      this._lastManagedAction = action;
    }
    if (action.kind === "device-login") {
      return this.refreshDeviceLoginCode(action);
    }
    return this._executeManagedAction(action);
  }

  async refreshDeviceLoginCode(action = null) {
    const oauthKind = (action && action.oauthKind) || this._oauthKind();
    if (!oauthKind) return null;
    this._codeRefreshBusy = true;
    if (this._runState?.state === "timeout" && this._runState.action?.kind === "device-login") {
      this._runState = null;
    }
    this._render();
    try {
      const result = await this._request("POST", this._endpoint("oauthStart", { kind: oauthKind }), this._collectConfig());
      this._lastResult = result;
      this._emit("result", { result });
      await this.refresh();
      this._lastManagedAction = this._currentAction();
      this._codeRefreshBusy = false;
      this._render();
      return result;
    } catch (error) {
      this._codeRefreshBusy = false;
      this._error(error);
      this._render();
      return null;
    }
  }

  async _executeManagedAction(action) {
    const before = this._snapshot();
    let waitAction = action;
    this._runState = { state: "running", action: waitAction, before };
    this._deviceLoginRunning = waitAction.kind === "device-login";
    this._render();
    this._emit("action-start", { action: waitAction, before });
    try {
      if (waitAction.kind === "continue") {
        const result = await this._request("POST", this._endpoint("next"), this._collectConfig());
        this._lastResult = result;
        this._emit("result", { result });
        if (result && result.setup_status) {
          this._applyState(result);
          this._emit("state", { state: this._state });
          this._render();
          const after = this._snapshot();
          if (this._advanced(before, after, waitAction)) {
            this._runState = null;
            this._deviceLoginRunning = false;
            this._emit("action-complete", { action: waitAction, state: result });
            this._render();
            return result;
          }
        }
        this._maybeOpenDeviceLogin(result);
        if (result && result.ok === false) {
          await this.refresh();
          this._runState = null;
          this._deviceLoginRunning = false;
          this._render();
          return this._state;
        }
        const login = findDeviceLogin(result);
        if (login && login.url) {
          waitAction = {
            kind: "device-login",
            oauthKind: this._oauthKind(),
            label: this._t("openDeviceLogin"),
            url: login.url
          };
          this._lastManagedAction = waitAction;
          this._runState = { state: "running", action: waitAction, before };
          this._deviceLoginRunning = true;
          this._render();
        }
      } else if (waitAction.kind === "add-to-teams" && waitAction.url) {
        this._emitSkipNext("manual-add-to-teams", waitAction);
        window.open(waitAction.url, "_blank", "noopener");
        this._manualActions.addToTeamsOpened = true;
        this._writeManualActions();
        this._runState = null;
        this._deviceLoginRunning = false;
        this._emit("action-complete", { action: waitAction, state: this._state });
        this._render();
        return this._state;
      } else if (waitAction.kind === "refresh" || waitAction.kind === "blocked-refresh") {
        this._emitSkipNext("refresh-only", waitAction);
        await this.refresh();
      } else if (waitAction.kind === "download-package" && waitAction.url) {
        this._emitSkipNext("manual-download-package", waitAction);
        window.open(waitAction.url, "_blank", "noopener");
        this._runState = null;
        this._render();
        return this._state;
      } else if (waitAction.url) {
        this._emitSkipNext("manual-url-action", waitAction);
        window.open(waitAction.url, "_blank", "noopener");
      }
      const state = await this._waitForAdvance(before, waitAction);
      this._runState = null;
      this._deviceLoginRunning = false;
      this._emit("action-complete", { action: waitAction, state });
      this._render();
      return state;
    } catch (error) {
      this._deviceLoginRunning = false;
      this._runState = { state: "timeout", action: waitAction, before, error };
      this._emit("action-timeout", { action: waitAction, error });
      this._render();
      return null;
    }
  }

  async _postAndRefresh(path, body, pollOAuth) {
    this._setBusy(true);
    try {
      let result = await this._request("POST", path, body);
      this._lastResult = result;
      this._emit("result", { result });
      this._maybeOpenDeviceLogin(result);
      this._render();
      if (pollOAuth) {
        for (let attempts = 0; attempts < 180 && pendingDeviceLogin(result); attempts += 1) {
          const login = findDeviceLogin(result);
          const interval = Math.max(5, Number(login && login.interval || 5)) * 1000;
          await new Promise((resolve) => setTimeout(resolve, interval));
          result = await this._request("POST", this._endpoint("next"), this._collectConfig());
          this._lastResult = result;
          this._emit("result", { result });
          this._render();
        }
      }
      await this.refresh();
      return result;
    } catch (error) {
      this._lastResult = { ok: false, error: error.message };
      this._error(error);
      throw error;
    } finally {
      this._setBusy(false);
    }
  }

  async _request(method, path, body) {
    const options = { method, headers: {} };
    if (body) {
      options.headers["Content-Type"] = "application/json";
      options.body = JSON.stringify(body);
    }
    const url = href(this.apiBase, path);
    this._emit("request-start", { method, path, url });
    let response;
    try {
      response = await fetch(url, options);
    } catch (error) {
      this._emit("request-error", { method, path, url, message: error.message || String(error) });
      throw error;
    }
    const text = await response.text();
    let payload = {};
    if (text) {
      try {
        payload = JSON.parse(text);
      } catch {
        payload = { raw: text };
      }
    }
    if (!response.ok) {
      const error = new Error(payload.error || payload.message || `${response.status} ${response.statusText}`);
      error.payload = payload;
      this._emit("request-error", { method, path, url, status: response.status, message: error.message, payload });
      throw error;
    }
    this._emit("request-success", { method, path, url, status: response.status });
    return payload;
  }

  async _loadState() {
    const state = await this._request("GET", this._endpoint("state"));
    this._applyState(state);
    this._error(null);
    this._emit("state", { state: this._state });
    this._render();
    return this._state;
  }

  async _waitForAdvance(before, action) {
    const started = Date.now();
    const deadline = this._actionDeadline(started, action);
    while (Date.now() < deadline) {
      if (action.kind === "device-login") {
        try {
          const result = await this._request("POST", this._endpoint("oauthComplete", { kind: action.oauthKind || this._oauthKind() }), this._collectConfig());
          this._lastResult = result;
          this._emit("result", { result });
          if (result && result.ok) {
            await this._request("POST", this._endpoint("next"), this._collectConfig());
            this._lastResult = null;
          } else if (deviceCodeInvalid(result)) {
            await this.refreshDeviceLoginCode(action);
          } else if (!authorizationPending(result)) {
            this._runState = null;
            this._deviceLoginRunning = false;
            this._render();
            return this._state;
          }
        } catch {
          // Keep polling until timeout; the visible timeout state handles failure.
        }
      }
      await this._sleep(action.kind === "device-login" ? this._deviceLoginPollMs() : Math.min(this.pollInterval, 3000));
      const state = await this._loadState();
      const after = this._snapshot();
      if (this._advanced(before, after, action)) {
        return state;
      }
    }
    throw new Error(this._t("stepTimedOut"));
  }

  _sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  _manualActionStorageKey() {
    const statePath = this._endpoint("state");
    return `greentic-teams-setup:${this.providerId}:${this.apiBase}:${statePath}`;
  }

  _readManualActions() {
    try {
      return JSON.parse(window.sessionStorage.getItem(this._manualActionStorageKey()) || "{}") || {};
    } catch {
      return {};
    }
  }

  _writeManualActions() {
    try {
      window.sessionStorage.setItem(this._manualActionStorageKey(), JSON.stringify(this._manualActions));
    } catch {
      // Session storage is only an affordance for browser refreshes; setup still works without it.
    }
  }

  _onClick(event) {
    const action = event.target && event.target.closest("[data-action]");
    if (!action) return;
    if (action.dataset.action === "refresh") this.refresh();
    if (action.dataset.action === "run-current") this.runCurrentAction();
    if (action.dataset.action === "retry-current") this.retryCurrentAction();
    if (action.dataset.action === "refresh-code") this.refreshDeviceLoginCode();
    if (action.dataset.action === "open-device-login") this._openDeviceLogin();
    if (action.dataset.action === "next") this.runNextStep();
    if (action.dataset.action === "save") this.saveConfiguration();
    if (action.dataset.action === "copy-code") this._copyCode(action.dataset.code || "");
  }

  _openDeviceLogin() {
    const login = this._activeDeviceLogin();
    if (login && login.url) {
      window.open(login.url, "_blank", "noopener");
      this._emit("device-login", { login });
    }
  }

  _onInput(event) {
    const input = event.target && event.target.closest("[data-field]");
    if (!input) return;
    this._draftConfig[input.dataset.field] = input.value;
  }

  _copyCode(code) {
    if (!code) return;
    navigator.clipboard && navigator.clipboard.writeText(code);
    this._copyMessage = this._t("codeCopied");
    this._emit("copy-code", { code });
    this._renderOAuth();
    window.setTimeout(() => {
      this._copyMessage = "";
      this._renderOAuth();
    }, 1800);
  }

  _maybeOpenDeviceLogin(result) {
    const login = findDeviceLogin(result);
    if (!login || !login.url || this._oauthComplete(this._oauthKind())) return;
    this._emit("device-login", { login });
  }

  _configurePolling() {
    this._stopPolling();
    if (this.autoPoll) {
      this._pollTimer = window.setInterval(() => this.refresh(), this.pollInterval);
    }
  }

  _stopPolling() {
    if (this._pollTimer) {
      window.clearInterval(this._pollTimer);
      this._pollTimer = 0;
    }
  }

  _setBusy(value) {
    this.shadowRoot.querySelectorAll("button").forEach((button) => {
      const keepEnabled = this._deviceLoginRunning
        && (button.dataset.action === "refresh-code" || button.dataset.action === "open-device-login" || button.dataset.action === "copy-code");
      button.disabled = !keepEnabled && (value || this._runState?.state === "running");
    });
  }

  _error(error) {
    const target = this.shadowRoot.querySelector('[data-role="error"]');
    if (!target) return;
    if (!error) {
      target.hidden = true;
      target.textContent = "";
      return;
    }
    target.hidden = false;
    target.innerHTML = `
      <h3>${this._escape(this._t("errorTitle"))}</h3>
      <p>${this._escape(error.message || "")}</p>
    `;
    this._emit("error", { error });
  }

  _localError(message) {
    const target = this.shadowRoot.querySelector('[data-role="error"]');
    if (!target) return;
    target.hidden = false;
    target.innerHTML = `
      <h3>${this._escape(this._t("errorTitle"))}</h3>
      <p>${this._escape(message || "")}</p>
    `;
  }

  _render() {
    this._renderStaticText();
    this._renderProgress();
    this._renderOutcome();
    this._renderNextAction();
    this._renderRunStatus();
    this._renderOAuth();
    this._renderBlocked();
    this._renderActions();
    this._renderSteps();
    this._renderAdvanced();
    this._renderConfigValues();
    this._renderLastResult();
  }

  _renderStaticText() {
    this.shadowRoot.querySelectorAll("[data-i18n]").forEach((node) => {
      node.textContent = this._t(node.dataset.i18n);
    });
  }

  _renderProgress() {
    const status = this._status();
    const items = status.items || [];
    const done = items.filter((item) => item.state === "done").length;
    const blocked = items.some((item) => item.state === "blocked") || Boolean(status.blocked);
    const total = items.length || 1;
    const pct = Math.round((done / total) * 100);
    const overall = this.shadowRoot.querySelector('[data-role="overall"]');
    const bar = this.shadowRoot.querySelector('[data-role="progress"]');
    const text = this.shadowRoot.querySelector('[data-role="progressText"]');
    if (bar) bar.style.setProperty("--progress", `${pct}%`);
    if (text) text.textContent = this._t("progress").replace("{done}", String(done)).replace("{total}", String(items.length || 0));
    if (!overall) return;
    const complete = done === items.length && items.length > 0;
    const loaded = items.length > 0;
    overall.className = `pill ${blocked ? "blocked" : complete ? "done" : "pending"}`;
    overall.textContent = blocked
      ? this._t("blocked")
      : complete
        ? this._t("complete")
        : done > 0
          ? this._t("inProgress")
          : loaded
            ? this._t("ready")
            : this._t("loading");
  }

  _renderNextAction() {
    const target = this.shadowRoot.querySelector('[data-role="next"]');
    if (!target) return;
    const status = this._status();
    target.textContent = status.next || this._t("noNext");
  }

  _renderOutcome() {
    const target = this.shadowRoot.querySelector('[data-role="outcome"]');
    if (!target) return;
    const result = this._latestSetupResult();
    const message = this._outcomeMessage(result);
    if (!message) {
      target.hidden = true;
      target.textContent = "";
      return;
    }
    target.hidden = false;
    const title = result && result.ok === false ? this._t("errorTitle") : this._t("completedTitle");
    target.innerHTML = `
      <h3>${this._escape(title)}</h3>
      <p>${this._escape(message)}</p>
    `;
  }

  _renderRunStatus() {
    const target = this.shadowRoot.querySelector('[data-role="runStatus"]');
    if (!target) return;
    if (!this._runState) {
      target.hidden = true;
      target.textContent = "";
      return;
    }
    target.hidden = false;
    if (this._runState.state === "timeout") {
      target.className = "callout error";
      const detail = this._runState.error && this._runState.error.message
        ? this._runState.error.message
        : this._t("stepTimedOut");
      target.innerHTML = `
        <h3>${this._escape(this._t("stepTimedOutTitle"))}</h3>
        <p>${this._escape(detail)}</p>
      `;
      return;
    }
    target.className = "callout running";
    target.innerHTML = `
      <h3>${this._escape(this._t("working"))}</h3>
      <p>${this._escape(this._t("waitingForNextStep"))}</p>
    `;
  }

  _renderOAuth() {
    const target = this.shadowRoot.querySelector('[data-role="oauth"]');
    if (!target) return;
    const login = this._activeDeviceLogin();
    if (!login) {
      target.hidden = true;
      target.textContent = "";
      return;
    }
    target.hidden = false;
    target.innerHTML = `
      <h3>${this._escape(this._t("oauthTitle"))}</h3>
      <p>${this._escape(this._t("oauthInstruction"))}</p>
      ${login.userCode ? `
        <div class="buttons">
          <span class="code">${this._escape(login.userCode)}</span>
          <button type="button" data-action="copy-code" data-code="${this._escape(login.userCode)}">${this._escape(this._t("copyCode"))}</button>
        </div>
      ` : ""}
      ${this._copyMessage ? `<p class="muted">${this._escape(this._copyMessage)}</p>` : ""}
    `;
  }

  _pendingLoginFromState() {
    const cfg = this._config();
    const values = this._state && this._state.values || {};
    const response = values.last_oauth && values.last_oauth.response || {};
    const userCode = cfg.oauth_user_code || response.user_code || response.userCode;
    if (!userCode) return null;
    return {
      url: cfg.oauth_verification_uri || response.verification_uri || response.verification_url || "https://login.microsoft.com/device",
      userCode,
      message: response.message || "",
      interval: Number(response.interval || 5),
      expiresIn: Number(response.expires_in || 900)
    };
  }

  _activeDeviceLogin() {
    if (this._currentPendingStepId() !== "graph_admin_consent") return null;
    const login = findDeviceLogin(this._lastResult) || this._pendingLoginFromState();
    if (!login || this._oauthComplete(this._oauthKind())) return null;
    return login;
  }

  _staleManagedAction(action) {
    if (!action) return true;
    if (action.kind === "device-login") {
      return this._currentPendingStepId() !== "graph_admin_consent" || this._oauthComplete(action.oauthKind || this._oauthKind());
    }
    return false;
  }

  _renderBlocked() {
    const target = this.shadowRoot.querySelector('[data-role="blocked"]');
    if (!target) return;
    const blocked = this._status().blocked;
    if (!blocked) {
      target.hidden = true;
      target.textContent = "";
      return;
    }
    target.hidden = false;
    target.innerHTML = `
      <h3>${this._escape(blocked.title || this._t("manualTitle"))}</h3>
      ${blocked.summary ? `<p>${this._escape(blocked.summary)}</p>` : ""}
      ${blocked.missing_action ? `<p>${this._escape(this._t("action"))}: <span class="code">${this._escape(blocked.missing_action)}</span></p>` : ""}
      ${blocked.recommended_role ? `<p>${this._escape(this._t("role"))}: ${this._escape(blocked.recommended_role)}</p>` : ""}
      ${blocked.recommended_scope ? `<p>${this._escape(this._t("scope"))}: <span class="code">${this._escape(blocked.recommended_scope)}</span></p>` : ""}
      ${blocked.next ? `<p>${this._escape(blocked.next)}</p>` : ""}
    `;
  }

  _renderActions() {
    const target = this.shadowRoot.querySelector('[data-role="actions"]');
    if (!target) return;
    if (this._deviceLoginRunning) {
      const secondary = `<button type="button" data-action="refresh-code" ${this._codeRefreshBusy ? "disabled" : ""}>${this._escape(this._codeRefreshBusy ? this._t("refreshingCode") : this._t("refreshCode"))}</button>`;
      target.innerHTML = `<button type="button" class="primary" disabled>${this._escape(this._t("working"))}</button><button type="button" data-action="open-device-login">${this._escape(this._t("openDeviceLogin"))}</button>${secondary}`;
      return;
    }
    if (this._runState?.state === "running") {
      const secondary = this._runState.action?.kind === "device-login"
        ? `<button type="button" data-action="refresh-code" ${this._codeRefreshBusy ? "disabled" : ""}>${this._escape(this._codeRefreshBusy ? this._t("refreshingCode") : this._t("refreshCode"))}</button>`
        : "";
      const openLogin = this._runState.action?.kind === "device-login"
        ? `<button type="button" data-action="open-device-login">${this._escape(this._t("openDeviceLogin"))}</button>`
        : "";
      target.innerHTML = `<button type="button" class="primary" disabled>${this._escape(this._t("working"))}</button>${openLogin}${secondary}`;
      return;
    }
    if (this._runState?.state === "timeout") {
      const label = this._runState.action?.kind === "device-login" ? this._t("refreshCode") : this._t("retry");
      target.innerHTML = `<button type="button" class="primary" data-action="retry-current">${this._escape(label)}</button>`;
      return;
    }
    const action = this._currentAction();
    if (!action) {
      this._emitSkipNext("render-no-current-action", action);
      target.innerHTML = "";
      return;
    }
    const secondary = action.kind === "device-login"
      ? `<button type="button" data-action="refresh-code" ${this._codeRefreshBusy ? "disabled" : ""}>${this._escape(this._codeRefreshBusy ? this._t("refreshingCode") : this._t("refreshCode"))}</button>`
      : "";
    target.innerHTML = `${this._actionHtml(action)}${secondary}`;
  }

  _currentAction() {
    const login = this._activeDeviceLogin();
    if (login && login.url) {
      return {
        kind: "device-login",
        oauthKind: this._oauthKind(),
        label: this._t("openDeviceLogin"),
        url: login.url
      };
    }

    const status = this._status();
    const items = status.items || [];
    if (status.blocked) {
      return {
        kind: "blocked-refresh",
        label: this._t("refreshAfterManualAction")
      };
    }

    const complete = items.length > 0 && items.every((item) => item.state === "done");
    const teams = this._state && this._state.teams_app || {};
    const values = this._state && this._state.values || {};
    const publish = values.last_teams_app_publish || {};
    const install = values.last_teams_app_install || {};
    const firstMessage = values.last_activity || values.last_webchat_conversation;

    if (publish.ok && !install.ok && teams.add_to_teams_url) {
      if (this._manualActions.addToTeamsOpened) {
        return {
          kind: "continue",
          label: this._t("verifyTeamsInstall")
        };
      }
      return {
        kind: "add-to-teams",
        label: this._t("addToTeams"),
        url: teams.add_to_teams_url
      };
    }

    if (!complete) {
      return {
        kind: "continue",
        label: this._t("continue") || "Continue setup"
      };
    }

    if (install.ok && !firstMessage && teams.open_bot_chat_url) {
      return {
        kind: "open-chat",
        label: this._t("openBotChat"),
        url: teams.open_bot_chat_url
      };
    }

    if (teams.open_bot_chat_url) {
      return {
        kind: "open-chat",
        label: this._t("openBotChat"),
        url: teams.open_bot_chat_url
      };
    }

    if (teams.ok) {
      return {
        kind: "download-package",
        label: this._t("downloadPackage"),
        url: href(this.apiBase, this._endpoint("package"))
      };
    }

    return {
      kind: "refresh",
      label: this._t("refresh")
    };
  }

  _currentPendingStepId() {
    const item = (this._status().items || []).find((entry) => entry.state !== "done");
    return item && item.id || "";
  }

  _preflightAction(action) {
    if (!action || action.kind !== "continue") return "";
    if (this._currentPendingStepId() !== "bot_framework_endpoint_registration") return "";
    const cfg = this._mergedConfig();
    if (!String(cfg.public_base_url || "").trim()) return this._t("missingPublicBaseUrl");
    if (!String(cfg.bot_framework_registration_url || "").trim()) return this._t("missingRegistrationService");
    return "";
  }

  _actionHtml(action) {
    return `<button type="button" class="primary" data-action="run-current">${this._escape(action.label || this._t("continue"))}</button>`;
  }

  _emitSkipNext(reason, action, extra = {}) {
    const status = this._status();
    this._emit("skip-next", {
      reason,
      pendingStepId: this._currentPendingStepId(),
      actionKind: action && action.kind || "",
      runState: this._runState && this._runState.state || "",
      next: status.next || "",
      ok: status.ok === true,
      blocked: Boolean(status.blocked),
      ...extra
    });
  }

  _renderSteps() {
    const target = this.shadowRoot.querySelector('[data-role="steps"]');
    if (!target) return;
    target.innerHTML = (this._status().items || []).map((item) => `
      <span class="pill ${this._escape(item.state || "pending")}">${this._escape(item.label || "")}</span>
    `).join("");
  }

  _renderAdvanced() {
    const details = this.shadowRoot.querySelector('[data-role="advancedDetails"]');
    if (details) details.hidden = !this.advanced;
  }

  _renderFields() {
    const target = this.shadowRoot.querySelector('[data-role="fields"]');
    if (!target) return;
    target.innerHTML = CONFIG_FIELDS.map((field) => `
      <label class="${field === "bot_app_password" ? "wide" : ""}">
        ${this._escape(this._fieldLabel(field))}
        <input data-field="${this._escape(field)}" type="${field.includes("password") ? "password" : "text"}">
      </label>
    `).join("");
  }

  _renderConfigValues() {
    const cfg = this._mergedConfig();
    this.shadowRoot.querySelectorAll("[data-field]").forEach((input) => {
      if (this.shadowRoot.activeElement === input) return;
      input.value = cfg[input.dataset.field] || "";
    });
  }

  _renderLastResult() {
    const target = this.shadowRoot.querySelector('[data-role="result"]');
    if (target) target.textContent = this._lastResult ? safeJson(this._lastResult) : "";
  }

  _latestSetupResult() {
    if (this._lastResult && this._lastResult.step) return this._lastResult;
    const values = this._state && this._state.values || {};
    return values.last_setup_result || null;
  }

  _outcomeMessage(result) {
    if (!result || typeof result !== "object") return "";
    if (pendingDeviceLogin(result)) return "";
    const bodyError = result.body && result.body.error;
    const resultBodyError = result.result && result.result.body && result.result.body.error;
    if (bodyError === "authorization_pending" || resultBodyError === "authorization_pending") {
      return "";
    }
    if (result.ok === false) {
      return result.next || result.error || this._t("actionFailed");
    }
    const step = result.step || "";
    const data = result.result && typeof result.result === "object" ? result.result : {};
    if (step === "bot_framework_endpoint_registration" || step === "reconcile_bot_framework_registration" || step === "reconcile_azure_bot_resource") {
      const registration = data.registration && data.registration.body || {};
      const action = data.action || registration.action || "";
      const endpoint = data.target_messaging_endpoint || data.current_messaging_endpoint || registration.target_messaging_endpoint || registration.current_messaging_endpoint || "";
      if (endpoint) {
        return action === "keep"
          ? `Bot endpoint already pointed at ${endpoint}.`
          : `Bot endpoint updated to ${endpoint}.`;
      }
    }
    if ((step === "teams_app_publish" || step === "publish_teams_app") && data.add_to_teams_url) {
      return `Teams app published. Add to Teams is ready.`;
    }
    if ((step === "teams_app_user_install" || step === "install_teams_app_for_user") && data.open_bot_chat_url) {
      return `Teams app installed. Bot chat is ready to open.`;
    }
    if (step === "bot_app_identity" || step === "create_or_reuse_bot_app") {
      const appId = data.app_id || data.bot_app_id || data.client_id;
      return appId ? `Bot app identity is ready: ${appId}.` : this._t("outcomes.create_or_reuse_bot_app");
    }
    return this._t(`outcomes.${step}`) || result.next || "";
  }

  _deviceLoginPollMs() {
    const login = this._activeDeviceLogin();
    return Math.max(5, Number(login && login.interval || 5)) * 1000;
  }

  _actionDeadline(started, action) {
    if (action.kind !== "device-login") return started + this.actionTimeout;
    const login = this._activeDeviceLogin();
    const expiresMs = Math.max(60, Number(login && login.expiresIn || 900)) * 1000;
    return started + Math.max(this.actionTimeout, expiresMs - 10000);
  }

  _collectConfig() {
    const config = this._mergedConfig();
    this.shadowRoot.querySelectorAll("[data-field]").forEach((input) => {
      config[input.dataset.field] = input.value;
    });
    const clientConfig = this._clientConfig(config);
    this._draftConfig = { ...this._draftConfig, ...clientConfig };
    return { config: clientConfig };
  }

  _clientConfig(config) {
    const serverOwned = new Set([
      "oauth_kind",
      "oauth_device_code",
      "oauth_user_code",
      "graph_access_token",
      "azure_management_access_token",
      "bot_access_token"
    ]);
    const clean = {};
    for (const [key, value] of Object.entries(config || {})) {
      if (serverOwned.has(key)) continue;
      clean[key] = value;
    }
    return clean;
  }

  _snapshot() {
    const status = this._status();
    const items = status.items || [];
    const values = this._state && this._state.values || {};
    const setup = values.last_setup_result || {};
    const action = this._currentAction();
    return {
      done: items.filter((item) => item.state === "done").length,
      total: items.length,
      blocked: Boolean(status.blocked),
      next: status.next || "",
      actionKind: action && action.kind || "",
      lastStep: setup.step || "",
      publishOk: Boolean((values.last_teams_app_publish || {}).ok),
      installOk: Boolean((values.last_teams_app_install || {}).ok),
      firstMessage: Boolean(values.last_activity || values.last_webchat_conversation)
    };
  }

  _advanced(before, after, action) {
    if (after.blocked && !before.blocked) return true;
    if (after.done > before.done) return true;
    if (after.done === before.done && action.kind === "device-login" && after.lastStep && after.lastStep !== before.lastStep) return true;
    if (after.actionKind && after.actionKind !== before.actionKind) return true;
    if (action.kind === "add-to-teams" && after.installOk) return true;
    if (action.kind === "open-chat" && after.firstMessage) return true;
    if (action.kind === "device-login" && after.actionKind !== "device-login") return true;
    if (action.kind === "refresh" || action.kind === "blocked-refresh") return true;
    return false;
  }

  _status() {
    return this._state && this._state.setup_status || {};
  }

  _applyState(nextState) {
    if (!nextState || !nextState.setup_status) return false;
    if (this._isStaleState(nextState)) return false;
    this._state = nextState;
    return true;
  }

  _stateRank(state) {
    const status = state && state.setup_status || {};
    const items = status.items || [];
    const values = state && state.values || {};
    return {
      done: items.filter((item) => item.state === "done").length,
      total: items.length,
      ok: status.ok === true,
      lastStep: values.last_setup_result && values.last_setup_result.step || status.last_step || ""
    };
  }

  _isStaleState(nextState) {
    if (!this._state) return false;
    const current = this._stateRank(this._state);
    const next = this._stateRank(nextState);
    if (next.total !== current.total) return false;
    if (next.done < current.done) return true;
    if (current.ok && !next.ok) return true;
    return false;
  }

  _config() {
    return this._state && this._state.values && this._state.values.config || {};
  }

  _mergedConfig() {
    return { ...this._config(), ...this._draftConfig };
  }

  _oauthKind() {
    const cfg = this._config();
    if (cfg.oauth_kind) return cfg.oauth_kind;
    const result = this._latestSetupResult() || this._lastResult || {};
    const step = result.step || "";
    if (step.includes("management") || step.includes("azure")) return "management";
    return "graph";
  }

  _oauthComplete(kind) {
    const values = this._state && this._state.values || {};
    const oauth = values.oauth || {};
    return Boolean(oauth[kind || "default"] && oauth[kind || "default"].ok);
  }

  _fieldLabel(field) {
    return this._t(`fields.${field}`) || field;
  }

  _t(key) {
    const locale = this.locale.toLowerCase();
    const language = locale.split("-")[0];
    const defaults = GreenticTeamsSetup.translations.en;
    const local = mergeDeep(defaults, GreenticTeamsSetup.translations[language] || {});
    const custom = mergeDeep(local, this._translations || {});
    return key.split(".").reduce((value, part) => value && value[part], custom) || "";
  }

  _escape(value) {
    return String(value == null ? "" : value).replace(/[&<>"']/g, (char) => ({
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;"
    }[char]));
  }

  _emit(name, detail) {
    const eventDetail = {
      providerId: this.providerId,
      ...(detail || {})
    };
    this.dispatchEvent(new CustomEvent(`greentic-teams-setup-${name}`, {
      bubbles: true,
      composed: true,
      detail: eventDetail
    }));
    this.dispatchEvent(new CustomEvent(`greentic-provider-setup-${name}`, {
      bubbles: true,
      composed: true,
      detail: eventDetail
    }));
  }

  _emitCompleteIfDone(state) {
    if (!state || state.setup_status?.ok !== true) return;
    const step = state.values?.last_setup_result?.step || "";
    const signature = `${this.providerId}:${step}:${state.values?.last_activity_received_at || ""}`;
    if (signature === this._lastCompleteSignature) return;
    this._lastCompleteSignature = signature;
    this._emit("complete", { state });
  }
}

if (!customElements.get("greentic-teams-setup")) {
  customElements.define("greentic-teams-setup", GreenticTeamsSetup);
}

if (!customElements.get("greentic-teams-setup-v4")) {
  customElements.define("greentic-teams-setup-v4", class extends GreenticTeamsSetup {});
}

var GreenticSso = (() => {
  var __defProp = Object.defineProperty;
  var __defProps = Object.defineProperties;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropDescs = Object.getOwnPropertyDescriptors;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getOwnPropSymbols = Object.getOwnPropertySymbols;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  var __propIsEnum = Object.prototype.propertyIsEnumerable;
  var __defNormalProp = (obj, key, value) => key in obj ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value }) : obj[key] = value;
  var __spreadValues = (a, b) => {
    for (var prop in b || (b = {}))
      if (__hasOwnProp.call(b, prop))
        __defNormalProp(a, prop, b[prop]);
    if (__getOwnPropSymbols)
      for (var prop of __getOwnPropSymbols(b)) {
        if (__propIsEnum.call(b, prop))
          __defNormalProp(a, prop, b[prop]);
      }
    return a;
  };
  var __spreadProps = (a, b) => __defProps(a, __getOwnPropDescs(b));
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };
  var __copyProps = (to, from, except, desc) => {
    if (from && typeof from === "object" || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
    }
    return to;
  };
  var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);
  var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== "symbol" ? key + "" : key, value);

  // tools/webchat-sso/entry.js
  var entry_exports = {};
  __export(entry_exports, {
    GreenticSsoError: () => GreenticSsoError,
    completeCallbackFromPopup: () => completeCallbackFromPopup,
    createGreenticSso: () => createGreenticSso,
    createGreenticWebchatSso: () => createGreenticWebchatSso,
    mintChatToken: () => mintChatToken
  });

  // node_modules/@greentic/sso/dist/index.js
  var GreenticSsoError = class _GreenticSsoError extends Error {
    constructor(code, message) {
      super(message != null ? message : code);
      __publicField(this, "code");
      this.name = "GreenticSsoError";
      this.code = code;
      Object.setPrototypeOf(this, _GreenticSsoError.prototype);
    }
  };
  var DEFAULT_SCOPE = "openid profile email";
  var TENANT_PATTERN = /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/;
  function isNonEmpty(value) {
    return typeof value === "string" && value.trim().length > 0;
  }
  function requireNonEmpty(value, field) {
    if (!isNonEmpty(value)) {
      throw new GreenticSsoError(`missing_${field}`, `GreenticSsoConfig.${field} is required`);
    }
    return value;
  }
  function resolveConfig(config) {
    var _a;
    const tenant = requireNonEmpty(config.tenant, "tenant");
    if (!TENANT_PATTERN.test(tenant)) {
      throw new GreenticSsoError(
        "invalid_tenant",
        `GreenticSsoConfig.tenant must match ${TENANT_PATTERN.source}, got ${JSON.stringify(tenant)}`
      );
    }
    const clientId = requireNonEmpty(config.clientId, "clientId");
    const redirectUri = requireNonEmpty(config.redirectUri, "redirectUri");
    const issuer = isNonEmpty(config.issuer) ? config.issuer : `https://${tenant}.greentic-id.com`;
    const scope = isNonEmpty(config.scope) ? config.scope : DEFAULT_SCOPE;
    const persist = (_a = config.persist) != null ? _a : false;
    return { tenant, issuer, clientId, redirectUri, scope, persist };
  }
  function buildAuthorizeUrl(cfg, params) {
    const url = new URL("/oauth/authorize", cfg.issuer);
    url.searchParams.set("response_type", "code");
    url.searchParams.set("client_id", cfg.clientId);
    url.searchParams.set("redirect_uri", cfg.redirectUri);
    url.searchParams.set("scope", cfg.scope);
    url.searchParams.set("code_challenge", params.codeChallenge);
    url.searchParams.set("code_challenge_method", "S256");
    url.searchParams.set("state", params.state);
    url.searchParams.set("nonce", params.nonce);
    return url.toString();
  }
  function isRecord(value) {
    return typeof value === "object" && value !== null;
  }
  function parseTokenSet(data) {
    if (!isRecord(data)) {
      throw new GreenticSsoError("token_exchange_failed", "Token response was not a JSON object");
    }
    const { access_token: accessToken, id_token: idToken, expires_in: expiresIn, token_type: tokenType, refresh_token: refreshToken } = data;
    if (typeof accessToken !== "string" || accessToken.length === 0) {
      throw new GreenticSsoError("token_exchange_failed", "Token response missing access_token");
    }
    if (typeof idToken !== "string" || idToken.length === 0) {
      throw new GreenticSsoError("token_exchange_failed", "Token response missing id_token");
    }
    if (typeof expiresIn !== "number" || !Number.isFinite(expiresIn)) {
      throw new GreenticSsoError("token_exchange_failed", "Token response missing or invalid expires_in");
    }
    if (typeof tokenType !== "string" || tokenType.length === 0) {
      throw new GreenticSsoError("token_exchange_failed", "Token response missing token_type");
    }
    const tokens = {
      accessToken,
      idToken,
      expiresIn,
      tokenType
    };
    if (typeof refreshToken === "string" && refreshToken.length > 0) {
      tokens.refreshToken = refreshToken;
    }
    return tokens;
  }
  async function exchangeCode(cfg, params) {
    const body = new URLSearchParams();
    body.set("grant_type", "authorization_code");
    body.set("code", params.code);
    body.set("redirect_uri", cfg.redirectUri);
    body.set("client_id", cfg.clientId);
    body.set("code_verifier", params.codeVerifier);
    const response = await fetch(`${cfg.issuer}/oauth/token`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString()
    });
    if (!response.ok) {
      throw new GreenticSsoError("token_exchange_failed", `Token exchange failed with HTTP ${response.status}`);
    }
    const data = await response.json();
    return parseTokenSet(data);
  }
  async function refreshTokens(cfg, refreshToken) {
    const body = new URLSearchParams();
    body.set("grant_type", "refresh_token");
    body.set("refresh_token", refreshToken);
    body.set("client_id", cfg.clientId);
    const response = await fetch(`${cfg.issuer}/oauth/token`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString()
    });
    if (!response.ok) {
      throw new GreenticSsoError("token_refresh_failed", `Token refresh failed with HTTP ${response.status}`);
    }
    const data = await response.json();
    return parseTokenSet(data);
  }
  function endSessionUrl(cfg, params = {}) {
    const url = new URL("/oauth/logout", cfg.issuer);
    if (isNonEmpty2(params.idTokenHint)) {
      url.searchParams.set("id_token_hint", params.idTokenHint);
    }
    if (isNonEmpty2(params.postLogoutRedirectUri)) {
      url.searchParams.set("post_logout_redirect_uri", params.postLogoutRedirectUri);
    }
    return url.toString();
  }
  function isNonEmpty2(value) {
    return typeof value === "string" && value.length > 0;
  }
  var DEFAULT_TIMEOUT_MS = 5 * 60 * 1e3;
  var CLOSED_POLL_MS = 500;
  var MESSAGE_TYPE = "greentic-sso";
  function openAuthPopup(url, opts) {
    const { expectedOrigin, expectedState, timeoutMs = DEFAULT_TIMEOUT_MS } = opts;
    return new Promise((resolve, reject) => {
      const popup = window.open(url, "greentic-sso", "width=480,height=640");
      if (!popup) {
        reject(new GreenticSsoError("popup_blocked", "window.open() returned null; the popup was likely blocked"));
        return;
      }
      let settled = false;
      const settle = (run) => {
        if (settled) return;
        settled = true;
        window.removeEventListener("message", onMessage);
        clearInterval(closedPoll);
        clearTimeout(timeoutHandle);
        run();
      };
      function onMessage(event) {
        if (event.origin !== expectedOrigin) return;
        const data = event.data;
        if (typeof data !== "object" || data === null) return;
        const record = data;
        if (record.type !== MESSAGE_TYPE) return;
        if (typeof record.state !== "string" || record.state !== expectedState) {
          settle(
            () => reject(new GreenticSsoError("state_mismatch", "Popup response state did not match the expected state"))
          );
          return;
        }
        if (typeof record.error === "string" && record.error.length > 0) {
          const errorCode = record.error;
          const errorDescription = typeof record.errorDescription === "string" ? record.errorDescription : void 0;
          settle(() => reject(new GreenticSsoError(errorCode, errorDescription)));
          return;
        }
        if (typeof record.code !== "string" || record.code.length === 0) {
          settle(
            () => reject(new GreenticSsoError("popup_response_invalid", "Popup response was missing a valid code"))
          );
          return;
        }
        const code = record.code;
        const state = record.state;
        settle(() => resolve({ code, state }));
      }
      window.addEventListener("message", onMessage);
      const closedPoll = setInterval(() => {
        if (popup.closed) {
          settle(
            () => reject(new GreenticSsoError("popup_closed", "The authentication popup was closed before completing"))
          );
        }
      }, CLOSED_POLL_MS);
      const timeoutHandle = setTimeout(() => {
        settle(
          () => reject(new GreenticSsoError("popup_timeout", "Timed out waiting for the authentication popup to complete"))
        );
        try {
          popup.close();
        } catch (e) {
        }
      }, timeoutMs);
    });
  }
  function base64UrlEncode(bytes) {
    let binary = "";
    for (const byte of bytes) {
      binary += String.fromCharCode(byte);
    }
    const base64 = btoa(binary);
    return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }
  function base64UrlDecodeToBytes(segment) {
    const normalized = segment.replace(/-/g, "+").replace(/_/g, "/");
    const paddingNeeded = (4 - normalized.length % 4) % 4;
    const padded = normalized + "=".repeat(paddingNeeded);
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
  function base64UrlDecodeToString(segment) {
    return new TextDecoder().decode(base64UrlDecodeToBytes(segment));
  }
  var CODE_VERIFIER_CHARSET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
  var MIN_VERIFIER_LENGTH = 43;
  var MAX_VERIFIER_LENGTH = 128;
  var DEFAULT_VERIFIER_LENGTH = 64;
  function generateCodeVerifier(length = DEFAULT_VERIFIER_LENGTH) {
    if (length < MIN_VERIFIER_LENGTH || length > MAX_VERIFIER_LENGTH) {
      throw new GreenticSsoError(
        "invalid_verifier_length",
        `code_verifier length must be between ${MIN_VERIFIER_LENGTH} and ${MAX_VERIFIER_LENGTH}, got ${length}`
      );
    }
    const randomBytes = new Uint8Array(length);
    crypto.getRandomValues(randomBytes);
    let verifier = "";
    for (const byte of randomBytes) {
      verifier += CODE_VERIFIER_CHARSET.charAt(byte % CODE_VERIFIER_CHARSET.length);
    }
    return verifier;
  }
  async function codeChallengeFromVerifier(verifier) {
    const data = new TextEncoder().encode(verifier);
    const digest = await crypto.subtle.digest("SHA-256", data);
    return base64UrlEncode(new Uint8Array(digest));
  }
  var DEFAULT_TOKEN_BYTE_LENGTH = 16;
  function randomUrlToken(byteLen = DEFAULT_TOKEN_BYTE_LENGTH) {
    if (byteLen < 1) {
      throw new GreenticSsoError("invalid_token_length", `byteLen must be >= 1, got ${byteLen}`);
    }
    const randomBytes = new Uint8Array(byteLen);
    crypto.getRandomValues(randomBytes);
    return base64UrlEncode(randomBytes);
  }
  function decodeIdTokenPayload(idToken) {
    const segments = idToken.split(".");
    const payloadSegment = segments[1];
    if (segments.length < 2 || payloadSegment === void 0 || payloadSegment.length === 0) {
      throw new GreenticSsoError("invalid_id_token", "id_token is not a valid JWT (missing payload segment)");
    }
    let claims;
    try {
      claims = JSON.parse(base64UrlDecodeToString(payloadSegment));
    } catch (e) {
      throw new GreenticSsoError("invalid_id_token", "id_token payload is not valid base64url JSON");
    }
    if (typeof claims !== "object" || claims === null) {
      throw new GreenticSsoError("invalid_id_token", "id_token payload is not a JSON object");
    }
    return claims;
  }
  function decodeIdentity(idToken) {
    const record = decodeIdTokenPayload(idToken);
    const sub = record.sub;
    if (typeof sub !== "string" || sub.length === 0) {
      throw new GreenticSsoError("invalid_id_token", "id_token payload missing sub claim");
    }
    const email = typeof record.email === "string" ? record.email : void 0;
    const name = typeof record.name === "string" ? record.name : void 0;
    const verifiedClaim = "verified" in record ? record.verified : record.email_verified;
    const verified = verifiedClaim === true;
    return { sub, email, name, verified };
  }
  function sessionFromTokens(tokens, nowMs) {
    const identity = decodeIdentity(tokens.idToken);
    const expiresAt = nowMs + tokens.expiresIn * 1e3;
    return { tokens, identity, expiresAt };
  }
  function isExpired(session, nowMs, skewMs = 0) {
    return nowMs >= session.expiresAt - skewMs;
  }
  function isTokenSet(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return typeof r.accessToken === "string" && typeof r.idToken === "string" && typeof r.expiresIn === "number" && typeof r.tokenType === "string";
  }
  function isIdentity(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return typeof r.sub === "string" && typeof r.verified === "boolean";
  }
  function isSession(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return isTokenSet(r.tokens) && isIdentity(r.identity) && typeof r.expiresAt === "number";
  }
  var STORAGE_KEY = "greentic-sso-session";
  function getSessionStorage() {
    try {
      const globalWithStorage = globalThis;
      return globalWithStorage.sessionStorage;
    } catch (e) {
      return void 0;
    }
  }
  var SessionStore = class {
    constructor(options = {}) {
      __publicField(this, "memorySession", null);
      __publicField(this, "persist");
      var _a;
      this.persist = (_a = options.persist) != null ? _a : false;
    }
    get() {
      if (this.persist) {
        const stored = this.readFromStorage();
        if (stored) return stored;
      }
      return this.memorySession;
    }
    set(session) {
      this.memorySession = session;
      if (this.persist) {
        this.writeToStorage(session);
      }
    }
    clear() {
      this.memorySession = null;
      if (this.persist) {
        this.clearStorage();
      }
    }
    readFromStorage() {
      const storage = getSessionStorage();
      if (!storage) return null;
      try {
        const raw = storage.getItem(STORAGE_KEY);
        if (!raw) return null;
        const parsed = JSON.parse(raw);
        return isSession(parsed) ? parsed : null;
      } catch (e) {
        return null;
      }
    }
    writeToStorage(session) {
      const storage = getSessionStorage();
      if (!storage) return;
      try {
        storage.setItem(STORAGE_KEY, JSON.stringify(session));
      } catch (e) {
      }
    }
    clearStorage() {
      const storage = getSessionStorage();
      if (!storage) return;
      try {
        storage.removeItem(STORAGE_KEY);
      } catch (e) {
      }
    }
  };
  function createGreenticSso(config) {
    const cfg = resolveConfig(config);
    const store = new SessionStore({ persist: cfg.persist });
    const subscribers = /* @__PURE__ */ new Set();
    function notify(identity) {
      for (const cb of subscribers) {
        try {
          cb(identity);
        } catch (e) {
        }
      }
    }
    async function login() {
      const verifier = generateCodeVerifier();
      const codeChallenge = await codeChallengeFromVerifier(verifier);
      const state = randomUrlToken();
      const nonce = randomUrlToken();
      const authorizeUrl = buildAuthorizeUrl(cfg, { codeChallenge, state, nonce });
      const { code } = await openAuthPopup(authorizeUrl, {
        expectedOrigin: new URL(cfg.redirectUri).origin,
        expectedState: state
      });
      const tokens = await exchangeCode(cfg, { code, codeVerifier: verifier });
      const idTokenNonce = decodeIdTokenPayload(tokens.idToken).nonce;
      if (typeof idTokenNonce !== "string" || idTokenNonce !== nonce) {
        throw new GreenticSsoError("nonce_mismatch", "id_token nonce did not match the request nonce");
      }
      const session = sessionFromTokens(tokens, Date.now());
      const identity = decodeIdentity(tokens.idToken);
      store.set(session);
      notify(identity);
      return identity;
    }
    async function logout(opts = {}) {
      const current = store.get();
      store.clear();
      notify(null);
      if (!opts.endSession) return;
      if (typeof window === "undefined") return;
      const url = endSessionUrl(cfg, {
        idTokenHint: current == null ? void 0 : current.tokens.idToken,
        postLogoutRedirectUri: cfg.redirectUri
      });
      window.location.href = url;
    }
    function getSession() {
      return store.get();
    }
    function onIdentity(cb) {
      subscribers.add(cb);
      return () => {
        subscribers.delete(cb);
      };
    }
    function isAuthenticated() {
      const session = store.get();
      return session !== null && !isExpired(session, Date.now());
    }
    async function getAccessToken() {
      const session = store.get();
      if (!session) {
        throw new GreenticSsoError("not_authenticated", "getAccessToken called with no active session");
      }
      const now = Date.now();
      if (!isExpired(session, now)) {
        return session.tokens.accessToken;
      }
      const refreshToken = session.tokens.refreshToken;
      if (!refreshToken) {
        throw new GreenticSsoError("session_expired", "Session expired and no refresh_token is available");
      }
      const refreshedTokens = await refreshTokens(cfg, refreshToken);
      const refreshedSession = sessionFromTokens(refreshedTokens, now);
      store.set(refreshedSession);
      return refreshedSession.tokens.accessToken;
    }
    function getIdToken() {
      const session = store.get();
      if (!session) {
        throw new GreenticSsoError("not_authenticated", "getIdToken called with no active session");
      }
      return session.tokens.idToken;
    }
    return { login, logout, getSession, onIdentity, getAccessToken, getIdToken, isAuthenticated };
  }
  function toParams(raw) {
    const stripped = raw.startsWith("?") || raw.startsWith("#") ? raw.slice(1) : raw;
    return new URLSearchParams(stripped);
  }
  function parseCallbackParams(search, hash) {
    const searchParams = toParams(search);
    const hashParams = toParams(hash);
    const pick = (name) => {
      const fromSearch = searchParams.get(name);
      if (fromSearch !== null) return fromSearch;
      const fromHash = hashParams.get(name);
      return fromHash !== null ? fromHash : void 0;
    };
    const code = pick("code");
    const state = pick("state");
    const error = pick("error");
    const errorDescription = pick("error_description");
    const result = {};
    if (code !== void 0) result.code = code;
    if (state !== void 0) result.state = state;
    if (error !== void 0) result.error = error;
    if (errorDescription !== void 0) result.errorDescription = errorDescription;
    return result;
  }
  function getOpener() {
    const opener = window.opener;
    return opener != null ? opener : null;
  }
  function completeCallbackFromPopup(opts = {}) {
    var _a;
    const { code, state, error, errorDescription } = parseCallbackParams(window.location.search, window.location.hash);
    const targetOrigin = (_a = opts.allowedOrigin) != null ? _a : window.location.origin;
    const opener = getOpener();
    if (opener) {
      const message = error !== void 0 ? { type: "greentic-sso", error, errorDescription, state } : { type: "greentic-sso", code, state };
      opener.postMessage(message, targetOrigin);
    }
    try {
      window.close();
    } catch (e) {
    }
  }

  // node_modules/@greentic/sso/dist/webchat/index.js
  var GreenticSsoError2 = class _GreenticSsoError2 extends Error {
    constructor(code, message) {
      super(message != null ? message : code);
      __publicField(this, "code");
      this.name = "GreenticSsoError";
      this.code = code;
      Object.setPrototypeOf(this, _GreenticSsoError2.prototype);
    }
  };
  var DEFAULT_SCOPE2 = "openid profile email";
  var TENANT_PATTERN2 = /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?$/;
  function isNonEmpty3(value) {
    return typeof value === "string" && value.trim().length > 0;
  }
  function requireNonEmpty2(value, field) {
    if (!isNonEmpty3(value)) {
      throw new GreenticSsoError2(`missing_${field}`, `GreenticSsoConfig.${field} is required`);
    }
    return value;
  }
  function resolveConfig2(config) {
    var _a;
    const tenant = requireNonEmpty2(config.tenant, "tenant");
    if (!TENANT_PATTERN2.test(tenant)) {
      throw new GreenticSsoError2(
        "invalid_tenant",
        `GreenticSsoConfig.tenant must match ${TENANT_PATTERN2.source}, got ${JSON.stringify(tenant)}`
      );
    }
    const clientId = requireNonEmpty2(config.clientId, "clientId");
    const redirectUri = requireNonEmpty2(config.redirectUri, "redirectUri");
    const issuer = isNonEmpty3(config.issuer) ? config.issuer : `https://${tenant}.greentic-id.com`;
    const scope = isNonEmpty3(config.scope) ? config.scope : DEFAULT_SCOPE2;
    const persist = (_a = config.persist) != null ? _a : false;
    return { tenant, issuer, clientId, redirectUri, scope, persist };
  }
  function buildAuthorizeUrl2(cfg, params) {
    const url = new URL("/oauth/authorize", cfg.issuer);
    url.searchParams.set("response_type", "code");
    url.searchParams.set("client_id", cfg.clientId);
    url.searchParams.set("redirect_uri", cfg.redirectUri);
    url.searchParams.set("scope", cfg.scope);
    url.searchParams.set("code_challenge", params.codeChallenge);
    url.searchParams.set("code_challenge_method", "S256");
    url.searchParams.set("state", params.state);
    url.searchParams.set("nonce", params.nonce);
    return url.toString();
  }
  function isRecord2(value) {
    return typeof value === "object" && value !== null;
  }
  function parseTokenSet2(data) {
    if (!isRecord2(data)) {
      throw new GreenticSsoError2("token_exchange_failed", "Token response was not a JSON object");
    }
    const { access_token: accessToken, id_token: idToken, expires_in: expiresIn, token_type: tokenType, refresh_token: refreshToken } = data;
    if (typeof accessToken !== "string" || accessToken.length === 0) {
      throw new GreenticSsoError2("token_exchange_failed", "Token response missing access_token");
    }
    if (typeof idToken !== "string" || idToken.length === 0) {
      throw new GreenticSsoError2("token_exchange_failed", "Token response missing id_token");
    }
    if (typeof expiresIn !== "number" || !Number.isFinite(expiresIn)) {
      throw new GreenticSsoError2("token_exchange_failed", "Token response missing or invalid expires_in");
    }
    if (typeof tokenType !== "string" || tokenType.length === 0) {
      throw new GreenticSsoError2("token_exchange_failed", "Token response missing token_type");
    }
    const tokens = {
      accessToken,
      idToken,
      expiresIn,
      tokenType
    };
    if (typeof refreshToken === "string" && refreshToken.length > 0) {
      tokens.refreshToken = refreshToken;
    }
    return tokens;
  }
  async function exchangeCode2(cfg, params) {
    const body = new URLSearchParams();
    body.set("grant_type", "authorization_code");
    body.set("code", params.code);
    body.set("redirect_uri", cfg.redirectUri);
    body.set("client_id", cfg.clientId);
    body.set("code_verifier", params.codeVerifier);
    const response = await fetch(`${cfg.issuer}/oauth/token`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString()
    });
    if (!response.ok) {
      throw new GreenticSsoError2("token_exchange_failed", `Token exchange failed with HTTP ${response.status}`);
    }
    const data = await response.json();
    return parseTokenSet2(data);
  }
  async function refreshTokens2(cfg, refreshToken) {
    const body = new URLSearchParams();
    body.set("grant_type", "refresh_token");
    body.set("refresh_token", refreshToken);
    body.set("client_id", cfg.clientId);
    const response = await fetch(`${cfg.issuer}/oauth/token`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: body.toString()
    });
    if (!response.ok) {
      throw new GreenticSsoError2("token_refresh_failed", `Token refresh failed with HTTP ${response.status}`);
    }
    const data = await response.json();
    return parseTokenSet2(data);
  }
  function endSessionUrl2(cfg, params = {}) {
    const url = new URL("/oauth/logout", cfg.issuer);
    if (isNonEmpty22(params.idTokenHint)) {
      url.searchParams.set("id_token_hint", params.idTokenHint);
    }
    if (isNonEmpty22(params.postLogoutRedirectUri)) {
      url.searchParams.set("post_logout_redirect_uri", params.postLogoutRedirectUri);
    }
    return url.toString();
  }
  function isNonEmpty22(value) {
    return typeof value === "string" && value.length > 0;
  }
  var DEFAULT_TIMEOUT_MS2 = 5 * 60 * 1e3;
  var CLOSED_POLL_MS2 = 500;
  var MESSAGE_TYPE2 = "greentic-sso";
  function openAuthPopup2(url, opts) {
    const { expectedOrigin, expectedState, timeoutMs = DEFAULT_TIMEOUT_MS2 } = opts;
    return new Promise((resolve, reject) => {
      const popup = window.open(url, "greentic-sso", "width=480,height=640");
      if (!popup) {
        reject(new GreenticSsoError2("popup_blocked", "window.open() returned null; the popup was likely blocked"));
        return;
      }
      let settled = false;
      const settle = (run) => {
        if (settled) return;
        settled = true;
        window.removeEventListener("message", onMessage);
        clearInterval(closedPoll);
        clearTimeout(timeoutHandle);
        run();
      };
      function onMessage(event) {
        if (event.origin !== expectedOrigin) return;
        const data = event.data;
        if (typeof data !== "object" || data === null) return;
        const record = data;
        if (record.type !== MESSAGE_TYPE2) return;
        if (typeof record.state !== "string" || record.state !== expectedState) {
          settle(
            () => reject(new GreenticSsoError2("state_mismatch", "Popup response state did not match the expected state"))
          );
          return;
        }
        if (typeof record.error === "string" && record.error.length > 0) {
          const errorCode = record.error;
          const errorDescription = typeof record.errorDescription === "string" ? record.errorDescription : void 0;
          settle(() => reject(new GreenticSsoError2(errorCode, errorDescription)));
          return;
        }
        if (typeof record.code !== "string" || record.code.length === 0) {
          settle(
            () => reject(new GreenticSsoError2("popup_response_invalid", "Popup response was missing a valid code"))
          );
          return;
        }
        const code = record.code;
        const state = record.state;
        settle(() => resolve({ code, state }));
      }
      window.addEventListener("message", onMessage);
      const closedPoll = setInterval(() => {
        if (popup.closed) {
          settle(
            () => reject(new GreenticSsoError2("popup_closed", "The authentication popup was closed before completing"))
          );
        }
      }, CLOSED_POLL_MS2);
      const timeoutHandle = setTimeout(() => {
        settle(
          () => reject(new GreenticSsoError2("popup_timeout", "Timed out waiting for the authentication popup to complete"))
        );
        try {
          popup.close();
        } catch (e) {
        }
      }, timeoutMs);
    });
  }
  function base64UrlEncode2(bytes) {
    let binary = "";
    for (const byte of bytes) {
      binary += String.fromCharCode(byte);
    }
    const base64 = btoa(binary);
    return base64.replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  }
  function base64UrlDecodeToBytes2(segment) {
    const normalized = segment.replace(/-/g, "+").replace(/_/g, "/");
    const paddingNeeded = (4 - normalized.length % 4) % 4;
    const padded = normalized + "=".repeat(paddingNeeded);
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }
  function base64UrlDecodeToString2(segment) {
    return new TextDecoder().decode(base64UrlDecodeToBytes2(segment));
  }
  var CODE_VERIFIER_CHARSET2 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
  var MIN_VERIFIER_LENGTH2 = 43;
  var MAX_VERIFIER_LENGTH2 = 128;
  var DEFAULT_VERIFIER_LENGTH2 = 64;
  function generateCodeVerifier2(length = DEFAULT_VERIFIER_LENGTH2) {
    if (length < MIN_VERIFIER_LENGTH2 || length > MAX_VERIFIER_LENGTH2) {
      throw new GreenticSsoError2(
        "invalid_verifier_length",
        `code_verifier length must be between ${MIN_VERIFIER_LENGTH2} and ${MAX_VERIFIER_LENGTH2}, got ${length}`
      );
    }
    const randomBytes = new Uint8Array(length);
    crypto.getRandomValues(randomBytes);
    let verifier = "";
    for (const byte of randomBytes) {
      verifier += CODE_VERIFIER_CHARSET2.charAt(byte % CODE_VERIFIER_CHARSET2.length);
    }
    return verifier;
  }
  async function codeChallengeFromVerifier2(verifier) {
    const data = new TextEncoder().encode(verifier);
    const digest = await crypto.subtle.digest("SHA-256", data);
    return base64UrlEncode2(new Uint8Array(digest));
  }
  var DEFAULT_TOKEN_BYTE_LENGTH2 = 16;
  function randomUrlToken2(byteLen = DEFAULT_TOKEN_BYTE_LENGTH2) {
    if (byteLen < 1) {
      throw new GreenticSsoError2("invalid_token_length", `byteLen must be >= 1, got ${byteLen}`);
    }
    const randomBytes = new Uint8Array(byteLen);
    crypto.getRandomValues(randomBytes);
    return base64UrlEncode2(randomBytes);
  }
  function decodeIdTokenPayload2(idToken) {
    const segments = idToken.split(".");
    const payloadSegment = segments[1];
    if (segments.length < 2 || payloadSegment === void 0 || payloadSegment.length === 0) {
      throw new GreenticSsoError2("invalid_id_token", "id_token is not a valid JWT (missing payload segment)");
    }
    let claims;
    try {
      claims = JSON.parse(base64UrlDecodeToString2(payloadSegment));
    } catch (e) {
      throw new GreenticSsoError2("invalid_id_token", "id_token payload is not valid base64url JSON");
    }
    if (typeof claims !== "object" || claims === null) {
      throw new GreenticSsoError2("invalid_id_token", "id_token payload is not a JSON object");
    }
    return claims;
  }
  function decodeIdentity2(idToken) {
    const record = decodeIdTokenPayload2(idToken);
    const sub = record.sub;
    if (typeof sub !== "string" || sub.length === 0) {
      throw new GreenticSsoError2("invalid_id_token", "id_token payload missing sub claim");
    }
    const email = typeof record.email === "string" ? record.email : void 0;
    const name = typeof record.name === "string" ? record.name : void 0;
    const verifiedClaim = "verified" in record ? record.verified : record.email_verified;
    const verified = verifiedClaim === true;
    return { sub, email, name, verified };
  }
  function sessionFromTokens2(tokens, nowMs) {
    const identity = decodeIdentity2(tokens.idToken);
    const expiresAt = nowMs + tokens.expiresIn * 1e3;
    return { tokens, identity, expiresAt };
  }
  function isExpired2(session, nowMs, skewMs = 0) {
    return nowMs >= session.expiresAt - skewMs;
  }
  function isTokenSet2(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return typeof r.accessToken === "string" && typeof r.idToken === "string" && typeof r.expiresIn === "number" && typeof r.tokenType === "string";
  }
  function isIdentity2(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return typeof r.sub === "string" && typeof r.verified === "boolean";
  }
  function isSession2(value) {
    if (typeof value !== "object" || value === null) return false;
    const r = value;
    return isTokenSet2(r.tokens) && isIdentity2(r.identity) && typeof r.expiresAt === "number";
  }
  var STORAGE_KEY2 = "greentic-sso-session";
  function getSessionStorage2() {
    try {
      const globalWithStorage = globalThis;
      return globalWithStorage.sessionStorage;
    } catch (e) {
      return void 0;
    }
  }
  var SessionStore2 = class {
    constructor(options = {}) {
      __publicField(this, "memorySession", null);
      __publicField(this, "persist");
      var _a;
      this.persist = (_a = options.persist) != null ? _a : false;
    }
    get() {
      if (this.persist) {
        const stored = this.readFromStorage();
        if (stored) return stored;
      }
      return this.memorySession;
    }
    set(session) {
      this.memorySession = session;
      if (this.persist) {
        this.writeToStorage(session);
      }
    }
    clear() {
      this.memorySession = null;
      if (this.persist) {
        this.clearStorage();
      }
    }
    readFromStorage() {
      const storage = getSessionStorage2();
      if (!storage) return null;
      try {
        const raw = storage.getItem(STORAGE_KEY2);
        if (!raw) return null;
        const parsed = JSON.parse(raw);
        return isSession2(parsed) ? parsed : null;
      } catch (e) {
        return null;
      }
    }
    writeToStorage(session) {
      const storage = getSessionStorage2();
      if (!storage) return;
      try {
        storage.setItem(STORAGE_KEY2, JSON.stringify(session));
      } catch (e) {
      }
    }
    clearStorage() {
      const storage = getSessionStorage2();
      if (!storage) return;
      try {
        storage.removeItem(STORAGE_KEY2);
      } catch (e) {
      }
    }
  };
  function createGreenticSso2(config) {
    const cfg = resolveConfig2(config);
    const store = new SessionStore2({ persist: cfg.persist });
    const subscribers = /* @__PURE__ */ new Set();
    function notify(identity) {
      for (const cb of subscribers) {
        try {
          cb(identity);
        } catch (e) {
        }
      }
    }
    async function login() {
      const verifier = generateCodeVerifier2();
      const codeChallenge = await codeChallengeFromVerifier2(verifier);
      const state = randomUrlToken2();
      const nonce = randomUrlToken2();
      const authorizeUrl = buildAuthorizeUrl2(cfg, { codeChallenge, state, nonce });
      const { code } = await openAuthPopup2(authorizeUrl, {
        expectedOrigin: new URL(cfg.redirectUri).origin,
        expectedState: state
      });
      const tokens = await exchangeCode2(cfg, { code, codeVerifier: verifier });
      const idTokenNonce = decodeIdTokenPayload2(tokens.idToken).nonce;
      if (typeof idTokenNonce !== "string" || idTokenNonce !== nonce) {
        throw new GreenticSsoError2("nonce_mismatch", "id_token nonce did not match the request nonce");
      }
      const session = sessionFromTokens2(tokens, Date.now());
      const identity = decodeIdentity2(tokens.idToken);
      store.set(session);
      notify(identity);
      return identity;
    }
    async function logout(opts = {}) {
      const current = store.get();
      store.clear();
      notify(null);
      if (!opts.endSession) return;
      if (typeof window === "undefined") return;
      const url = endSessionUrl2(cfg, {
        idTokenHint: current == null ? void 0 : current.tokens.idToken,
        postLogoutRedirectUri: cfg.redirectUri
      });
      window.location.href = url;
    }
    function getSession() {
      return store.get();
    }
    function onIdentity(cb) {
      subscribers.add(cb);
      return () => {
        subscribers.delete(cb);
      };
    }
    function isAuthenticated() {
      const session = store.get();
      return session !== null && !isExpired2(session, Date.now());
    }
    async function getAccessToken() {
      const session = store.get();
      if (!session) {
        throw new GreenticSsoError2("not_authenticated", "getAccessToken called with no active session");
      }
      const now = Date.now();
      if (!isExpired2(session, now)) {
        return session.tokens.accessToken;
      }
      const refreshToken = session.tokens.refreshToken;
      if (!refreshToken) {
        throw new GreenticSsoError2("session_expired", "Session expired and no refresh_token is available");
      }
      const refreshedTokens = await refreshTokens2(cfg, refreshToken);
      const refreshedSession = sessionFromTokens2(refreshedTokens, now);
      store.set(refreshedSession);
      return refreshedSession.tokens.accessToken;
    }
    function getIdToken() {
      const session = store.get();
      if (!session) {
        throw new GreenticSsoError2("not_authenticated", "getIdToken called with no active session");
      }
      return session.tokens.idToken;
    }
    return { login, logout, getSession, onIdentity, getAccessToken, getIdToken, isAuthenticated };
  }
  function isRecord22(value) {
    return typeof value === "object" && value !== null;
  }
  function parseChatToken(data) {
    if (!isRecord22(data)) {
      throw new GreenticSsoError2("chat_token_failed", "Chat token response was not a JSON object");
    }
    const { token, expires_in: expiresIn } = data;
    if (typeof token !== "string" || token.length === 0) {
      throw new GreenticSsoError2("chat_token_failed", "Chat token response missing token");
    }
    if (typeof expiresIn !== "number" || !Number.isFinite(expiresIn)) {
      throw new GreenticSsoError2("chat_token_failed", "Chat token response missing or invalid expires_in");
    }
    return { token, expiresIn };
  }
  async function mintChatToken(chatApiBase, accessToken) {
    const response = await fetch(`${chatApiBase}/token`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${accessToken}`,
        Accept: "application/json"
      }
    });
    if (response.status === 401) {
      throw new GreenticSsoError2("unauthorized", "Chat token mint rejected the access token (401)");
    }
    if (!response.ok) {
      throw new GreenticSsoError2("chat_token_failed", `Chat token mint failed with HTTP ${response.status}`);
    }
    const data = await response.json();
    return parseChatToken(data);
  }
  var CHAT_TOKEN_REFRESH_MARGIN_MS = 3e4;
  var WEBCHAT_SCOPE = "greentic.webchat";
  function isNonEmpty32(value) {
    return typeof value === "string" && value.trim().length > 0;
  }
  function withWebchatScope(scope) {
    const base = isNonEmpty32(scope) ? scope : DEFAULT_SCOPE2;
    const tokens = base.split(/\s+/).filter((s) => s.length > 0);
    if (tokens.includes(WEBCHAT_SCOPE)) {
      return tokens.join(" ");
    }
    return [...tokens, WEBCHAT_SCOPE].join(" ");
  }
  function createGreenticWebchatSso(config) {
    const scope = withWebchatScope(config.scope);
    const resolved = resolveConfig2(__spreadProps(__spreadValues({}, config), { scope }));
    const chatApiBase = isNonEmpty32(config.chatApiBase) ? config.chatApiBase : `${resolved.issuer}/v1/messaging/webchat/${resolved.tenant}`;
    const client = createGreenticSso2(__spreadProps(__spreadValues({}, config), { scope }));
    let cachedChatToken = null;
    client.onIdentity(() => {
      cachedChatToken = null;
    });
    async function getChatToken() {
      const now = Date.now();
      if (cachedChatToken && cachedChatToken.expiresAt - now > CHAT_TOKEN_REFRESH_MARGIN_MS) {
        return cachedChatToken.token;
      }
      const accessToken = await client.getAccessToken();
      const minted = await mintChatToken(chatApiBase, accessToken);
      cachedChatToken = { token: minted.token, expiresAt: now + minted.expiresIn * 1e3 };
      return minted.token;
    }
    return __spreadProps(__spreadValues({}, client), { getChatToken });
  }
  return __toCommonJS(entry_exports);
})();

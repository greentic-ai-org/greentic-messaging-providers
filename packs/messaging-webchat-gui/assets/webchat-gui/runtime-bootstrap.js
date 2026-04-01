console.log('[runtime-bootstrap] loaded');
(function () {
  var SUPPORTED_LOCALES = {
    'ar': 'العربية', 'ar-AE': 'العربية (الإمارات)', 'ar-DZ': 'العربية (الجزائر)',
    'ar-EG': 'العربية (مصر)', 'ar-IQ': 'العربية (العراق)', 'ar-MA': 'العربية (المغرب)',
    'ar-SA': 'العربية (السعودية)', 'ar-SD': 'العربية (السودان)', 'ar-SY': 'العربية (سوريا)',
    'ar-TN': 'العربية (تونس)',
    'ay': 'Aymar aru',
    'bg': 'Български',
    'bn': 'বাংলা',
    'cs': 'Čeština',
    'da': 'Dansk',
    'de': 'Deutsch',
    'el': 'Ελληνικά',
    'en': 'English',
    'en-GB': 'English (UK)',
    'es': 'Español',
    'et': 'Eesti',
    'fa': 'فارسی',
    'fi': 'Suomi',
    'fr': 'Français',
    'gn': "Avañe'ẽ",
    'gu': 'ગુજરાતી',
    'hi': 'हिन्दी',
    'hr': 'Hrvatski',
    'ht': 'Kreyòl ayisyen',
    'hu': 'Magyar',
    'id': 'Bahasa Indonesia',
    'it': 'Italiano',
    'ja': '日本語',
    'km': 'ខ្មែរ',
    'kn': 'ಕನ್ನಡ',
    'ko': '한국어',
    'lo': 'ລາວ',
    'lt': 'Lietuvių',
    'lv': 'Latviešu',
    'ml': 'മലയാളം',
    'mr': 'मराठी',
    'ms': 'Bahasa Melayu',
    'my': 'မြန်မာ',
    'nah': 'Nāhuatl',
    'ne': 'नेपाली',
    'nl': 'Nederlands',
    'no': 'Norsk',
    'pa': 'ਪੰਜਾਬੀ',
    'pl': 'Polski',
    'pt': 'Português',
    'qu': 'Runa simi',
    'ro': 'Română',
    'ru': 'Русский',
    'si': 'සිංහල',
    'sk': 'Slovenčina',
    'sr': 'Српски',
    'sv': 'Svenska',
    'ta': 'தமிழ்',
    'te': 'తెలుగు',
    'th': 'ไทย',
    'tl': 'Tagalog',
    'tr': 'Türkçe',
    'uk': 'Українська',
    'ur': 'اردو',
    'vi': 'Tiếng Việt',
    'zh': '中文'
  };

  // ---------------------------------------------------------------------------
  // Tenant / env / locale resolution
  // ---------------------------------------------------------------------------

  function resolveTenant() {
    var match = window.location.pathname.match(/\/v1\/web\/webchat\/([^\/?#]+)/i);
    if (match && match[1]) {
      return decodeURIComponent(match[1]);
    }
    var queryTenant = new URLSearchParams(window.location.search).get('tenant');
    if (queryTenant) {
      return queryTenant;
    }
    return document.documentElement?.dataset?.tenant || 'default';
  }

  function resolveEnv() {
    var queryEnv = new URLSearchParams(window.location.search).get('env');
    if (queryEnv) {
      return queryEnv;
    }
    return document.documentElement?.dataset?.env || 'default';
  }

  function resolveLocale() {
    var queryLang = new URLSearchParams(window.location.search).get('lang');
    if (queryLang && SUPPORTED_LOCALES[queryLang]) {
      return queryLang;
    }
    return null;
  }

  function resolveGuiBase(tenant) {
    return '/v1/web/webchat/' + encodeURIComponent(tenant) + '/';
  }

  function backendBase(tenant) {
    return window.location.origin + '/v1/messaging/webchat/' + encodeURIComponent(tenant);
  }

  var tenant = resolveTenant();
  var env = resolveEnv();
  var selectedLocale = resolveLocale();
  var guiBase = resolveGuiBase(tenant);
  console.log('[runtime-bootstrap] tenant:', tenant, 'env:', env, 'locale:', selectedLocale || '(default)');

  document.documentElement.dataset.tenant = tenant;
  window.__TENANT__ = tenant;
  window.__BASE_PATH__ = guiBase;
  window.APP_CONFIG_BASE = './config';
  window.__WEBCHAT_GUI_BASE__ = guiBase;
  window.__WEBCHAT_BACKEND_BASE__ = backendBase(tenant);
  window.__SUPPORTED_LOCALES__ = SUPPORTED_LOCALES;
  window.__SELECTED_LOCALE__ = selectedLocale;

  // ---------------------------------------------------------------------------
  // UI i18n: load translations for chrome strings (Logout, WebChat title, etc.)
  // ---------------------------------------------------------------------------

  var UI_STRINGS = {};
  var UI_STRINGS_LOADED = false;
  var UI_STRINGS_CALLBACKS = [];

  function loadUiI18n(locale) {
    var lang = locale || 'en';
    var url = guiBase + 'i18n/' + lang + '.json';
    fetch(url).then(function (res) {
      if (!res.ok) {
        // Fall back to base language (ar-AE -> ar)
        var base = lang.split('-')[0];
        if (base !== lang) {
          return fetch(guiBase + 'i18n/' + base + '.json');
        }
        return fetch(guiBase + 'i18n/en.json');
      }
      return res;
    }).then(function (res) {
      if (res && res.ok) return res.json();
      return {};
    }).then(function (data) {
      UI_STRINGS = data || {};
      UI_STRINGS_LOADED = true;
      UI_STRINGS_CALLBACKS.forEach(function (cb) { cb(); });
      UI_STRINGS_CALLBACKS = [];
      applyUiTranslations();
    }).catch(function () {
      UI_STRINGS_LOADED = true;
      UI_STRINGS_CALLBACKS.forEach(function (cb) { cb(); });
      UI_STRINGS_CALLBACKS = [];
    });
  }

  function uiT(key, fallback) {
    return UI_STRINGS[key] || fallback || key;
  }

  function isRtlLocale(locale) {
    var base = (locale || '').split('-')[0];
    return ['ar', 'he', 'fa', 'ur'].indexOf(base) >= 0;
  }

  function applyUiTranslations() {
    // Set topbar title from skin brand.name, fall back to i18n, then 'AI Assistant'
    var titleEl = document.querySelector('.topbar__title');
    if (titleEl) {
      var brandName = (window.__SKIN__ && window.__SKIN__.brand && window.__SKIN__.brand.name) || '';
      titleEl.textContent = brandName || uiT('product.greentic.long', 'AI Assistant');
    }
    // Translate logout button if already injected
    var logoutBtn = document.getElementById('greentic-logout-btn');
    if (logoutBtn) {
      logoutBtn.textContent = uiT('header.logout', 'Logout');
    }
    // Set lang/dir on html element for RTL locales
    var lang = selectedLocale || 'en';
    document.documentElement.lang = lang;
    var rtl = isRtlLocale(lang);
    document.documentElement.dir = rtl ? 'rtl' : 'ltr';

    // Inject RTL CSS for Adaptive Card content when locale is RTL
    if (rtl && !document.getElementById('greentic-rtl-style')) {
      var style = document.createElement('style');
      style.id = 'greentic-rtl-style';
      style.textContent = [
        '[dir="rtl"] .ac-adaptiveCard, [dir="rtl"] .ac-container { direction: rtl; text-align: right; }',
        '[dir="rtl"] .ac-textBlock { direction: rtl; text-align: right; }',
        '[dir="rtl"] .ac-actionSet { direction: rtl; }',
        '[dir="rtl"] .ac-input { direction: rtl; text-align: right; }',
        '[dir="rtl"] .webchat__bubble__content { direction: rtl; }',
        '[dir="rtl"] .webchat__stacked-layout { direction: rtl; }',
        '[dir="rtl"] .topbar { flex-direction: row-reverse; }',
        '[dir="rtl"] .topbar__brand { flex-direction: row-reverse; }',
      ].join('\n');
      document.head.appendChild(style);
    }
  }

  loadUiI18n(selectedLocale);

  // ---------------------------------------------------------------------------
  // OAuth helper functions
  // ---------------------------------------------------------------------------

  var OAUTH_STORAGE_PREFIX = 'greentic_oauth_';

  function oauthStorageKey(key) {
    return OAUTH_STORAGE_PREFIX + key;
  }

  function getOAuthSession() {
    try {
      var handle = sessionStorage.getItem(oauthStorageKey('token_handle'));
      var flowId = sessionStorage.getItem(oauthStorageKey('flow_id'));
      if (handle && flowId) {
        return { token_handle: handle, flow_id: flowId };
      }
    } catch (_) { /* sessionStorage unavailable */ }
    return null;
  }

  function saveOAuthSession(tokenHandle, flowId) {
    try {
      sessionStorage.setItem(oauthStorageKey('token_handle'), tokenHandle);
      sessionStorage.setItem(oauthStorageKey('flow_id'), flowId);
    } catch (_) { /* sessionStorage unavailable */ }
  }

  function clearOAuthSession() {
    try {
      sessionStorage.removeItem(oauthStorageKey('token_handle'));
      sessionStorage.removeItem(oauthStorageKey('flow_id'));
      sessionStorage.removeItem(oauthStorageKey('user_name'));
      sessionStorage.removeItem(oauthStorageKey('user_email'));
      sessionStorage.removeItem(oauthStorageKey('user_picture'));
      sessionStorage.removeItem(oauthStorageKey('provider'));
    } catch (_) { /* sessionStorage unavailable */ }
  }

  /**
   * Check URL for OAuth callback params (?code=...&state=...).
   * If found, exchange code for tokens via PKCE, save session, clean URL.
   * Returns true if callback was detected (async exchange happens in background).
   */
  function handleOAuthCallback() {
    var params = new URLSearchParams(window.location.search);
    var code = params.get('code');
    var state = params.get('state');
    var error = params.get('error');

    if (error) {
      var errorDesc = params.get('error_description') || error;
      console.error('[oauth] provider returned error:', errorDesc);
      cleanCallbackParams(params);
      showAuthError('Authentication failed: ' + errorDesc);
      return true;
    }

    if (!code || !state) {
      return false;
    }

    // Verify state matches what we stored
    var storedState;
    try { storedState = sessionStorage.getItem(oauthStorageKey('state')); } catch (_) {}
    if (storedState && storedState !== state) {
      console.error('[oauth] state mismatch');
      cleanCallbackParams(params);
      showAuthError('Authentication failed: invalid state. Please try again.');
      return true;
    }

    cleanCallbackParams(params);

    // Exchange code for tokens via server-side proxy (avoids CORS)
    var codeVerifier, redirectUri, storedProvider;
    try {
      codeVerifier = sessionStorage.getItem(oauthStorageKey('code_verifier'));
      redirectUri = sessionStorage.getItem(oauthStorageKey('redirect_uri'));
      var providerStr = sessionStorage.getItem(oauthStorageKey('provider'));
      if (providerStr) storedProvider = JSON.parse(providerStr);
    } catch (_) {}

    if (!storedProvider || !storedProvider.token_url || !storedProvider.client_id) {
      // Fallback: save as authenticated without user info
      console.log('[oauth] no provider info, saving basic session');
      saveOAuthSession('authenticated', 'oauth-code');
      removeOAuthOverlay();
      injectLogoutButton();
      return true;
    }

    var proxyUrl = backendBase(tenant) + '/oauth/token-exchange';
    console.log('[oauth] exchanging code via proxy');

    fetch(proxyUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        token_url: storedProvider.token_url,
        code: code,
        redirect_uri: redirectUri || window.location.href.split('?')[0],
        client_id: storedProvider.client_id,
        client_secret: storedProvider.client_secret || '',
        code_verifier: codeVerifier || ''
      })
    })
      .then(function (resp) { return resp.json(); })
      .then(function (tokens) {
        if (tokens.error) {
          throw new Error(tokens.error_description || tokens.error);
        }
        // Decode id_token JWT payload (base64url) to get user info
        var userInfo = {};
        if (tokens.id_token) {
          try {
            var parts = tokens.id_token.split('.');
            var payload = JSON.parse(atob(parts[1].replace(/-/g, '+').replace(/_/g, '/')));
            userInfo = {
              name: payload.name || '',
              email: payload.email || '',
              picture: payload.picture || ''
            };
          } catch (_) {}
        }
        console.log('[oauth] authenticated:', userInfo.name || userInfo.email || 'user');
        saveOAuthSession(tokens.id_token || tokens.access_token || 'authenticated', 'oauth-code');
        try {
          if (userInfo.name) sessionStorage.setItem(oauthStorageKey('user_name'), userInfo.name);
          if (userInfo.email) sessionStorage.setItem(oauthStorageKey('user_email'), userInfo.email);
          if (userInfo.picture) sessionStorage.setItem(oauthStorageKey('user_picture'), userInfo.picture);
          sessionStorage.removeItem(oauthStorageKey('code_verifier'));
          sessionStorage.removeItem(oauthStorageKey('redirect_uri'));
          sessionStorage.removeItem(oauthStorageKey('state'));
        } catch (_) {}
        removeOAuthOverlay();
        injectLogoutButton();
      })
      .catch(function (err) {
        console.warn('[oauth] token exchange failed, saving basic session:', err.message);
        saveOAuthSession('authenticated', 'oauth-code');
        removeOAuthOverlay();
        injectLogoutButton();
      });

    return true;
  }

  function cleanCallbackParams(params) {
    params.delete('code');
    params.delete('state');
    params.delete('error');
    params.delete('error_description');
    var cleanUrl = window.location.pathname;
    var remaining = params.toString();
    if (remaining) {
      cleanUrl += '?' + remaining;
    }
    window.history.replaceState({}, '', cleanUrl);
  }

  /**
   * Generate a random string for PKCE code verifier.
   */
  function generateCodeVerifier() {
    var array = new Uint8Array(32);
    crypto.getRandomValues(array);
    return btoa(String.fromCharCode.apply(null, array))
      .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }

  /**
   * Compute S256 code challenge from a code verifier.
   */
  async function generateCodeChallenge(verifier) {
    var data = new TextEncoder().encode(verifier);
    var digest = await crypto.subtle.digest('SHA-256', data);
    return btoa(String.fromCharCode.apply(null, new Uint8Array(digest)))
      .replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  }

  /**
   * Initiate OAuth flow by building the authorize URL directly (PKCE).
   * No server-side call needed — standard SPA OIDC flow.
   */
  /**
   * Initiate OAuth for a specific provider.
   * Provider object: { id, label, auth_url, token_url, client_id, scopes }
   */
  function initiateOAuthFlow(provider) {
    if (!provider.auth_url || !provider.client_id) {
      showAuthError('OAuth not configured for ' + (provider.label || provider.id) + '. Set auth_url and client_id.');
      return;
    }

    // Use clean URL without query params — Google requires exact redirect_uri match
    var redirectUri = window.location.href.split('?')[0];

    var codeVerifier = generateCodeVerifier();
    try {
      sessionStorage.setItem(oauthStorageKey('code_verifier'), codeVerifier);
      sessionStorage.setItem(oauthStorageKey('redirect_uri'), redirectUri);
      // Store which provider was used so callback knows token_url + client_id
      sessionStorage.setItem(oauthStorageKey('provider'), JSON.stringify(provider));
    } catch (_) {}

    var scopes = provider.scopes || 'openid profile email';
    var state = 'webchat-' + Date.now() + '-' + Math.random().toString(36).slice(2, 8);

    try {
      sessionStorage.setItem(oauthStorageKey('state'), state);
    } catch (_) {}

    generateCodeChallenge(codeVerifier).then(function (codeChallenge) {
      var params = new URLSearchParams({
        response_type: 'code',
        client_id: provider.client_id,
        redirect_uri: redirectUri,
        scope: scopes,
        state: state,
        code_challenge: codeChallenge,
        code_challenge_method: 'S256',
        access_type: 'offline',
        prompt: 'select_account'
      });

      var authorizeUrl = provider.auth_url + '?' + params.toString();
      console.log('[oauth] redirecting to provider:', provider.id, provider.auth_url);
      window.location.href = authorizeUrl;
    });
  }

  // Map provider IDs to friendly display names
  var PROVIDER_LABELS = {
    'oauth-oidc-generic': 'SSO',
    'generic_oidc': 'SSO',
    'google': 'Google',
    'microsoft': 'Microsoft',
    'msgraph': 'Microsoft',
    'github': 'GitHub',
    'apple': 'Apple',
    'okta': 'Okta',
    'auth0': 'Auth0',
    'keycloak': 'Keycloak'
  };

  function providerLabel(providerId) {
    return PROVIDER_LABELS[providerId] || providerId;
  }

  function performLogout() {
    clearOAuthSession();
    window.location.reload();
  }

  /**
   * Remove any existing OAuth overlay.
   */
  function removeOAuthOverlay() {
    var existing = document.getElementById('greentic-oauth-overlay');
    if (existing) existing.remove();
  }

  /**
   * Show a fullscreen login overlay with buttons for each OAuth provider.
   */
  function showLoginScreen(authConfig) {
    removeOAuthOverlay();
    var providers = (authConfig && authConfig.providers) || [];
    var overlay = document.createElement('div');
    overlay.id = 'greentic-oauth-overlay';
    overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;align-items:center;justify-content:center;background:#f8fafb;font-family:Poppins,system-ui,-apple-system,sans-serif;';
    var card = document.createElement('div');
    card.style.cssText = 'max-width:380px;width:90%;padding:48px 36px;border-radius:20px;box-shadow:0 8px 32px rgba(0,0,0,0.06);text-align:center;background:#fff;border:1px solid #e5e7eb;';
    // Logo icon
    var logoWrap = document.createElement('div');
    logoWrap.style.cssText = 'width:56px;height:56px;border-radius:50%;background:#ecfdf5;display:flex;align-items:center;justify-content:center;margin:0 auto 20px;';
    logoWrap.innerHTML = '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="#059669" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/></svg>';
    card.appendChild(logoWrap);
    var titleEl = document.createElement('h2');
    titleEl.textContent = 'Welcome';
    titleEl.style.cssText = 'margin:0 0 6px;font-size:1.375rem;font-weight:600;color:#1f2937;';
    card.appendChild(titleEl);
    var descEl = document.createElement('p');
    descEl.textContent = 'Sign in to start chatting';
    descEl.style.cssText = 'margin:0 0 32px;color:#6b7280;font-size:0.875rem;line-height:1.5;';
    card.appendChild(descEl);
    var btnContainer = document.createElement('div');
    btnContainer.style.cssText = 'display:flex;flex-direction:column;gap:10px;';
    providers.forEach(function (provider) {
      var label = provider.label || providerLabel(provider.id) || 'SSO';
      var displayLabel = /^(sign in|log in|continue)/i.test(label) ? label : 'Sign in with ' + label;
      var isDummy = provider.type === 'dummy';
      var btn = document.createElement('button');
      btn.textContent = displayLabel;
      if (isDummy) {
        btn.style.cssText = 'padding:12px 24px;border:1px solid #e5e7eb;border-radius:12px;background:#fff;color:#1f2937;font-size:0.875rem;font-weight:500;font-family:inherit;cursor:pointer;transition:all .15s;min-width:200px;';
        btn.onmouseover = function () { btn.style.borderColor = '#059669'; btn.style.color = '#059669'; };
        btn.onmouseout = function () { btn.style.borderColor = '#e5e7eb'; btn.style.color = '#1f2937'; };
      } else {
        btn.style.cssText = 'padding:12px 24px;border:none;border-radius:12px;background:#059669;color:#fff;font-size:0.875rem;font-weight:500;font-family:inherit;cursor:pointer;transition:all .15s;min-width:200px;';
        btn.onmouseover = function () { btn.style.background = '#047857'; };
        btn.onmouseout = function () { btn.style.background = '#059669'; };
      }
      btn.onclick = function () {
        btn.disabled = true;
        btn.textContent = 'Redirecting...';
        btn.style.opacity = '0.6';
        initiateOAuthFlow(provider);
      };
      btnContainer.appendChild(btn);
    });
    if (providers.length === 0) {
      var noP = document.createElement('p');
      noP.textContent = 'No sign-in providers configured.';
      noP.style.cssText = 'color:#6b7280;font-size:0.8125rem;';
      card.appendChild(noP);
    }
    card.appendChild(btnContainer);
    overlay.appendChild(card);
    document.body.appendChild(overlay);
  }

  function showAuthError(message) {
    removeOAuthOverlay();
    var overlay = document.createElement('div');
    overlay.id = 'greentic-oauth-overlay';
    overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;align-items:center;justify-content:center;background:#f8fafb;font-family:Poppins,system-ui,-apple-system,sans-serif;';
    var card = document.createElement('div');
    card.style.cssText = 'max-width:380px;width:90%;padding:48px 36px;border-radius:20px;box-shadow:0 8px 32px rgba(0,0,0,0.06);text-align:center;background:#fff;border:1px solid #e5e7eb;';
    var iconWrap = document.createElement('div');
    iconWrap.style.cssText = 'width:56px;height:56px;border-radius:50%;background:#fef2f2;display:flex;align-items:center;justify-content:center;margin:0 auto 20px;';
    iconWrap.innerHTML = '<svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="#ef4444" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/></svg>';
    card.appendChild(iconWrap);
    var titleEl = document.createElement('h2');
    titleEl.textContent = 'Something went wrong';
    titleEl.style.cssText = 'margin:0 0 6px;font-size:1.375rem;font-weight:600;color:#1f2937;';
    card.appendChild(titleEl);
    var descEl = document.createElement('p');
    descEl.textContent = message;
    descEl.style.cssText = 'margin:0 0 32px;color:#6b7280;font-size:0.875rem;line-height:1.5;';
    card.appendChild(descEl);
    var retryBtn = document.createElement('button');
    retryBtn.textContent = 'Try Again';
    retryBtn.style.cssText = 'padding:12px 24px;border:none;border-radius:12px;background:#059669;color:#fff;font-size:0.875rem;font-weight:500;font-family:inherit;cursor:pointer;transition:all .15s;min-width:200px;';
    retryBtn.onmouseover = function () { retryBtn.style.background = '#047857'; };
    retryBtn.onmouseout = function () { retryBtn.style.background = '#059669'; };
    retryBtn.onclick = function () {
      clearOAuthSession();
      window.location.reload();
    };
    card.appendChild(retryBtn);
    overlay.appendChild(card);
    document.body.appendChild(overlay);
  }

  /**
   * Inject logout button into the existing header bar (next to locale picker).
   * Uses MutationObserver to wait for the header to render.
   */
  // Flag: should inject logout when locale picker mounts
  window.__OAUTH_SHOW_LOGOUT__ = false;

  function injectLogoutButton() {
    window.__OAUTH_SHOW_LOGOUT__ = true;
    // If locale picker already mounted, inject now
    var container = document.getElementById('greentic-header-controls');
    if (container && !document.getElementById('greentic-logout-btn')) {
      appendLogoutToContainer(container);
    }
  }

  function appendLogoutToContainer(container) {
    // Show user avatar + name if available
    var userName, userPicture;
    try {
      userName = sessionStorage.getItem(oauthStorageKey('user_name'));
      userPicture = sessionStorage.getItem(oauthStorageKey('user_picture'));
    } catch (_) {}

    if (userPicture) {
      var avatar = document.createElement('img');
      avatar.src = userPicture;
      avatar.referrerPolicy = 'no-referrer';
      avatar.style.cssText = 'width:24px;height:24px;border-radius:50%;object-fit:cover;';
      container.appendChild(avatar);
    }

    if (userName) {
      var nameEl = document.createElement('span');
      nameEl.textContent = userName;
      nameEl.style.cssText = 'font-size:12px;color:#555;max-width:120px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
      container.appendChild(nameEl);
    }

    var btn = document.createElement('button');
    btn.textContent = uiT('header.logout', 'Logout');
    btn.id = 'greentic-logout-btn';
    btn.style.cssText = 'padding:4px 12px;border:1px solid #ccc;border-radius:4px;background:#fff;color:#555;font-size:12px;cursor:pointer;transition:background .15s;white-space:nowrap;';
    btn.onmouseover = function () { btn.style.background = '#f0f0f0'; };
    btn.onmouseout = function () { btn.style.background = '#fff'; };
    btn.onclick = performLogout;
    container.appendChild(btn);
  }

  // ---------------------------------------------------------------------------
  // OAuth gate: fetch auth config and gate the SPA if needed
  // ---------------------------------------------------------------------------

  // Cache for auth config (fetched once)
  window.__OAUTH_CONFIG__ = null;
  window.__OAUTH_CHECKED__ = false;

  /**
   * Fetch OAuth config from the backend /auth/config endpoint.
   * Blocks SPA rendering until auth is resolved.
   */
  function checkOAuthGate() {
    var authConfigUrl = backendBase(tenant) + '/auth/config';
    fetch(authConfigUrl)
      .then(function (response) {
        if (!response.ok) {
          console.log('[oauth] auth/config not available, skipping auth gate');
          window.__OAUTH_CONFIG__ = { enabled: false };
          window.__OAUTH_CHECKED__ = true;
          return;
        }
        return response.json();
      })
      .then(function (authConfig) {
        if (!authConfig) return;
        window.__OAUTH_CONFIG__ = authConfig;
        window.__OAUTH_CHECKED__ = true;
        console.log('[oauth] auth config:', authConfig.enabled ? 'enabled' : 'disabled');

        if (!authConfig.enabled) {
          return; // No auth required, SPA proceeds normally
        }

        // Step 1: Check if returning from OAuth callback (?code=...&state=...)
        if (handleOAuthCallback()) {
          // Callback detected — token exchange in progress
          return;
        }

        // Step 2: Check if we already have a valid session
        var session = getOAuthSession();
        if (session) {
          console.log('[oauth] existing session found');
          injectLogoutButton();
          return;
        }

        // Step 3: No session, show login screen
        console.log('[oauth] no session, showing login');
        showLoginScreen(authConfig);
      })
      .catch(function (err) {
        console.warn('[oauth] failed to fetch auth config, proceeding without auth:', err);
        window.__OAUTH_CONFIG__ = { enabled: false };
        window.__OAUTH_CHECKED__ = true;
      });
  }

  // Run OAuth check immediately
  checkOAuthGate();

  // ---------------------------------------------------------------------------
  // Fetch interceptor (tenant config + skin.json patching)
  // ---------------------------------------------------------------------------

  var originalFetch = window.fetch.bind(window);
  window.fetch = function (input, init) {
    var requestUrl = typeof input === 'string' ? input : input.url;
    var url = new URL(requestUrl, window.location.href);
    console.log('[bootstrap] fetch:', url.pathname);

    if (/\/config\/tenants\/[^/]+\.json$/i.test(url.pathname)) {
      var tenantId = decodeURIComponent(url.pathname.split('/').pop().replace(/\.json$/i, ''));
      var locale = selectedLocale || 'en-US';
      var authProviders = [
        {
          id: tenantId + '-demo',
          label: 'Demo Login',
          type: 'dummy',
          enabled: true
        }
      ];
      var payload = {
        tenant_id: tenantId,
        legacy_skin: '_template',
        branding: {
          company_name: tenantId
        },
        webchat: {
          directline: {
            token_url: window.location.origin + '/v1/messaging/webchat/' + encodeURIComponent(tenantId) + '/token',
            domain: window.location.origin + '/v1/messaging/webchat/' + encodeURIComponent(tenantId) + '/v3/directline'
          },
          locale: locale
        },
        auth: {
          providers: authProviders
        }
      };
      return Promise.resolve(
        new Response(JSON.stringify(payload), {
          status: 200,
          headers: { 'Content-Type': 'application/json' }
        })
      );
    }

    if (/skins\/[^/]+\/skin\.json$/i.test(url.pathname)) {
      return originalFetch(input, init).then(async function (response) {
        // Fallback: tenant skin not found or SPA fallback returned HTML
        var skinData;
        if (response.ok) {
          var contentType = response.headers.get('content-type') || '';
          if (contentType.includes('json')) {
            skinData = await response.json();
          } else {
            // SPA fallback returned HTML instead of JSON — skin doesn't exist
            response = null;
          }
        }
        if (!skinData) {
          var fallbackUrl = url.pathname.replace(/skins\/[^/]+\//, 'skins/_template/');
          console.log('[bootstrap] skin not found, falling back to _template:', fallbackUrl);
          var fbResponse = await originalFetch(fallbackUrl);
          if (!fbResponse.ok) return fbResponse;
          skinData = await fbResponse.json();
        }
        skinData.directLine = skinData.directLine || {};
        var ctxParams = 'env=' + encodeURIComponent(env) + '&tenant=' + encodeURIComponent(tenant);
        if (!skinData.directLine.tokenUrl) {
          skinData.directLine.tokenUrl = window.location.origin + '/v1/messaging/webchat/' + encodeURIComponent(tenant) + '/token?' + ctxParams;
        }
        if (!skinData.directLine.domain) {
          skinData.directLine.domain = window.location.origin + '/v1/messaging/webchat/' + encodeURIComponent(tenant) + '/v3/directline';
        }
        if (selectedLocale) {
          skinData.webchat = skinData.webchat || {};
          skinData.webchat.locale = selectedLocale;
        }
        skinData.statusBar = skinData.statusBar || {};
        skinData.statusBar.show = false;
        window.__SKIN__ = skinData;
        // Update topbar title with brand name from skin
        var titleEl = document.querySelector('.topbar__title');
        if (titleEl && skinData.brand && skinData.brand.name) {
          titleEl.textContent = skinData.brand.name;
        }
        console.log('[bootstrap] skin.json patched:', skinData.directLine, 'locale:', skinData.webchat?.locale);
        return new Response(JSON.stringify(skinData), {
          status: 200,
          headers: { 'Content-Type': 'application/json' }
        });
      });
    }

    return originalFetch(input, init);
  };

  // ---------------------------------------------------------------------------
  // Locale picker
  // ---------------------------------------------------------------------------

  // Fetch the flow pack's i18n manifest to determine which locales have
  // actual translations.  Only those locales appear in the picker.
  function fetchAvailableFlowLocales(callback) {
    // The i18n manifest lists locale codes that have card translations.
    // Served from the webchat-gui pack's i18n directory.
    var manifestUrl = guiBase + 'i18n/_manifest.json';
    fetch(manifestUrl)
      .then(function (res) { return res.ok ? res.json() : null; })
      .catch(function () { return null; })
      .then(function (codes) {
        if (Array.isArray(codes) && codes.length > 0) {
          var filtered = {};
          codes.forEach(function (code) {
            if (SUPPORTED_LOCALES[code]) {
              filtered[code] = SUPPORTED_LOCALES[code];
            }
          });
          if (!filtered['en']) filtered['en'] = 'English';
          callback(filtered);
        } else {
          // Manifest not available → show all supported locales
          callback(SUPPORTED_LOCALES);
        }
      });
  }

  // Build locale picker + logout button inside a single flex container
  function initLocalePicker(mountEl) {
    function buildPicker(locales) {
      // Create flex container that holds both locale picker and logout
      var container = document.createElement('div');
      container.id = 'greentic-header-controls';
      container.style.cssText = 'display:flex;align-items:center;gap:8px;';

      // Locale picker
      var globeSvg = '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10A15.3 15.3 0 0 1 12 2z"/></svg>';

      var wrapper = document.createElement('label');
      wrapper.className = 'locale-picker';
      wrapper.innerHTML = globeSvg;

      var selectEl = document.createElement('select');
      selectEl.className = 'locale-picker__select';

      var codes = Object.keys(locales).sort(function (a, b) {
        return locales[a].localeCompare(locales[b]);
      });
      var current = selectedLocale || 'en';
      codes.forEach(function (code) {
        var opt = document.createElement('option');
        opt.value = code;
        opt.textContent = locales[code] + ' (' + code + ')';
        if (code === current) opt.selected = true;
        selectEl.appendChild(opt);
      });

      selectEl.addEventListener('change', function () {
        var params = new URLSearchParams(window.location.search);
        params.set('lang', this.value);
        window.location.search = params.toString();
      });

      wrapper.appendChild(selectEl);
      container.appendChild(wrapper);

      // If OAuth is active and user is logged in, add logout button
      if (window.__OAUTH_SHOW_LOGOUT__) {
        appendLogoutToContainer(container);
      }

      mountEl.appendChild(container);
      console.log('[runtime-bootstrap] locale picker initialized, current:', current);
    }

    fetchAvailableFlowLocales(function (locales) {
      buildPicker(locales);
    });
  }

  // Use MutationObserver to detect when #locale-picker-mount appears in the DOM
  var pickerInitialized = false;
  var observer = new MutationObserver(function () {
    if (pickerInitialized) return;
    var mountEl = document.getElementById('locale-picker-mount');
    if (mountEl) {
      pickerInitialized = true;
      observer.disconnect();
      initLocalePicker(mountEl);
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
})();

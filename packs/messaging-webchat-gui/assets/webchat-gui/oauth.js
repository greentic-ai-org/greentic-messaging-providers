// Greentic WebChat OAuth Module
// Handles: session, login/error screens, OAuth flow, logout
(function () {
  var rt = window.__GTC_RUNTIME__ || {};
  var tenant = rt.tenant || 'demo';
  var backendBase = rt.backendBase || function(t) { return '/v1/messaging/webchat/' + t; };
  var originalFetch = rt.originalFetch || window.fetch.bind(window);
  var uiT = rt.uiT || function(k, fb) { return fb || k; };

  var OAUTH_STORAGE_PREFIX = 'greentic_oauth_';

  function oauthStorageKey(key) {
    return OAUTH_STORAGE_PREFIX + key;
  }

  function getOAuthSession() {
    try {
      // Check localStorage first (set by server-side callback), then sessionStorage
      var handle = localStorage.getItem(oauthStorageKey('token_handle')) || sessionStorage.getItem(oauthStorageKey('token_handle'));
      var flowId = localStorage.getItem(oauthStorageKey('flow_id')) || sessionStorage.getItem(oauthStorageKey('flow_id'));
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

    // Process tokens: decode id_token, save user info, update UI
    function handleTokens(tokens) {
      if (tokens.error) {
        throw new Error(tokens.error_description || tokens.error);
      }
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
    }

    // Direct PKCE token exchange with OAuth provider (no client_secret needed)
    function directTokenExchange() {
      if (!storedProvider.token_url) {
        throw new Error('no token_url');
      }
      console.log('[oauth] trying direct PKCE token exchange with', storedProvider.token_url);
      var body = new URLSearchParams({
        grant_type: 'authorization_code',
        code: code,
        redirect_uri: redirectUri || window.location.href.split('?')[0],
        client_id: storedProvider.client_id,
        code_verifier: codeVerifier || ''
      });
      return fetch(storedProvider.token_url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: body.toString()
      }).then(function (resp) { return resp.json(); });
    }

    // Try server proxy first, fall back to direct PKCE exchange
    var proxyUrl = backendBase(tenant) + '/oauth/token-exchange';
    console.log('[oauth] exchanging code via proxy');

    fetch(proxyUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        provider_id: storedProvider.id,
        token_url: storedProvider.token_url,
        code: code,
        redirect_uri: redirectUri || window.location.href.split('?')[0],
        client_id: storedProvider.client_id,
        code_verifier: codeVerifier || ''
      })
    })
      .then(function (resp) {
        if (!resp.ok) throw new Error('proxy returned ' + resp.status);
        return resp.json();
      })
      .then(handleTokens)
      .catch(function (proxyErr) {
        console.warn('[oauth] proxy failed:', proxyErr.message, '— trying direct PKCE exchange');
        directTokenExchange()
          .then(handleTokens)
          .catch(function (directErr) {
            console.warn('[oauth] direct exchange also failed:', directErr.message);
            saveOAuthSession('authenticated', 'oauth-code');
            removeOAuthOverlay();
            injectLogoutButton();
          });
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
    // Dummy/guest providers skip OAuth — just save session and proceed
    if (provider.type === 'dummy') {
      saveOAuthSession('guest', 'dummy');
      try {
        sessionStorage.setItem(oauthStorageKey('user_name'), 'Guest');
        sessionStorage.setItem(oauthStorageKey('provider'), JSON.stringify({ id: provider.id, type: 'dummy' }));
      } catch (_) {}
      removeOAuthOverlay();
      injectLogoutButton();
      return;
    }

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
          // If this is a fresh login (just redirected back), auto-send success
          // message to the flow so it routes to the next step.
          var justLoggedIn = sessionStorage.getItem(oauthStorageKey('just_logged_in'));
          if (justLoggedIn === 'true') {
            sessionStorage.removeItem(oauthStorageKey('just_logged_in'));
            console.log('[oauth] fresh login detected, sending success message to flow');
            // Wait for webchat to initialize, then send auto-message
            setTimeout(function () {
              sendOAuthSuccessToFlow();
            }, 2000);
          }
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

  /**
   * Handle hash-based OAuth success redirect from server-side callback.
   * URL format: /v1/web/webchat/demo/#oauth_success=true&access_token=...&state=...
   * This handles the case where the OAuth callback was processed server-side
   * (e.g., by greentic-start) and the user is redirected back with the token
   * in the URL hash fragment.
   */
  function handleHashOAuthCallback() {
    var hash = window.location.hash;
    if (!hash || !hash.includes('oauth_success=true')) {
      if (hash && hash.includes('oauth_error=')) {
        var errorParams = new URLSearchParams(hash.substring(1));
        var error = errorParams.get('oauth_error') || 'unknown error';
        window.location.hash = '';
        showAuthError('Authentication failed: ' + error);
        return true;
      }
      return false;
    }

    var params = new URLSearchParams(hash.substring(1));
    var accessToken = params.get('access_token') || '';
    var state = params.get('state') || '';

    // Clean hash from URL
    window.location.hash = '';
    window.history.replaceState({}, '', window.location.pathname + window.location.search);

    if (accessToken) {
      console.log('[oauth] server-side callback success, saving token');
      saveOAuthSession(accessToken, 'oauth-code');
      try {
        sessionStorage.setItem(oauthStorageKey('user_name'), 'GitHub User');
        sessionStorage.setItem(oauthStorageKey('provider'), JSON.stringify({ id: 'github', type: 'oauth' }));
      } catch (_) {}
      removeOAuthOverlay();
      injectLogoutButton();
      return true;
    }
    return false;
  }

  /**
   * Open OAuth login in a popup window. The callback page posts the token
   * back via postMessage, then we save it and notify the flow.
   */
  function openOAuthPopup(oauthUrl) {
    var w = 500, h = 600;
    var left = (screen.width - w) / 2;
    var top = (screen.height - h) / 2;
    var popup = window.open(oauthUrl, 'greentic_oauth', 'width=' + w + ',height=' + h + ',left=' + left + ',top=' + top);

    // Listen for postMessage from callback page
    function onMessage(event) {
      if (!event.data || event.data.type !== 'greentic_oauth_callback') return;
      window.removeEventListener('message', onMessage);
      if (event.data.status === 'success' && event.data.access_token) {
        console.log('[oauth] popup callback success');
        saveOAuthSession(event.data.access_token, 'oauth-code');
        try {
          sessionStorage.setItem(oauthStorageKey('user_name'), 'GitHub User');
          sessionStorage.setItem(oauthStorageKey('provider'), JSON.stringify({ id: 'github', type: 'oauth' }));
        } catch (_) {}
        removeOAuthOverlay();
        injectLogoutButton();
        // Dispatch custom event so webchat can send "oauth_login_success" to flow
        window.dispatchEvent(new CustomEvent('greentic_oauth_success', {
          detail: { access_token: event.data.access_token }
        }));
      } else {
        console.error('[oauth] popup callback error:', event.data.error);
        showAuthError('Authentication failed: ' + (event.data.error || 'unknown'));
      }
    }
    window.addEventListener('message', onMessage);

    // Fallback: poll popup closed (user closed manually)
    var pollTimer = setInterval(function () {
      if (popup && popup.closed) {
        clearInterval(pollTimer);
        window.removeEventListener('message', onMessage);
      }
    }, 1000);
  }

  /**
   * Send oauth_login_success by waiting for webchat to POST an activity
   * (any user message), then inject our message into the same conversation.
   * Uses fetch interception to capture the conversation URL.
   */
  var _oauthOrigFetch = window.fetch;
  var _oauthConvUrl = null;
  var _oauthToken = null;
  var _oauthNeedsSend = false;

  function sendOAuthSuccessToFlow() {
    _oauthNeedsSend = true;
    console.log('[oauth] will send oauth_login_success on next conversation activity');
    // Also try to find conversation from existing fetch traffic
    // Poll for 10 seconds waiting for webchat to establish conversation
    var attempts = 0;
    var poll = setInterval(function () {
      attempts++;
      if (_oauthConvUrl && _oauthToken) {
        clearInterval(poll);
        doSendOAuthSuccess();
      }
      if (attempts > 20) {
        clearInterval(poll);
        console.warn('[oauth] timed out waiting for conversation');
      }
    }, 500);
  }

  function doSendOAuthSuccess() {
    if (!_oauthConvUrl) return;
    console.log('[oauth] sending oauth_login_success to', _oauthConvUrl);
    _oauthOrigFetch(_oauthConvUrl, {
      method: 'POST',
      headers: {
        'Authorization': 'Bearer ' + _oauthToken,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        type: 'message',
        from: { id: 'oauth_user' },
        text: 'oauth_login_success'
      })
    }).then(function () {
      console.log('[oauth] oauth_login_success sent!');
      _oauthNeedsSend = false;
    }).catch(function (err) {
      console.warn('[oauth] send failed:', err);
    });
  }

  // Intercept fetch to capture conversation URL from webchat SDK activity posts
  var _prevFetch = window.fetch;
  window.fetch = function (url, opts) {
    var urlStr = typeof url === 'string' ? url : (url && url.url) || '';
    // Capture conversation activities URL and auth token
    if (/\/v3\/directline\/conversations\/[^/]+\/activities/i.test(urlStr)) {
      _oauthConvUrl = urlStr;
      if (opts && opts.headers) {
        var auth = opts.headers['Authorization'] || opts.headers['authorization'];
        if (auth) _oauthToken = auth.replace('Bearer ', '');
      }
      // If we need to send and this is a GET (polling), send now
      if (_oauthNeedsSend && opts && (!opts.method || opts.method === 'GET')) {
        doSendOAuthSuccess();
      }
    }
    return _prevFetch.apply(this, arguments);
  };

  // Expose for webchat card action interception
  window.__GREENTIC_OPEN_OAUTH_POPUP__ = openOAuthPopup;

  /**
   * Watch for "Login with OAuth" buttons rendered in Adaptive Cards.
   * Hijack their click to open a popup instead of submitting to the flow.
   */
  // Global click interceptor at document level (capture phase).
  // Fires BEFORE any WebChat SDK handler, so we can intercept OAuth clicks.
  document.addEventListener('click', function (e) {
    var target = e.target.closest('button, a, [role="button"]');
    if (!target) return;
    var text = (target.textContent || '').trim();
    var href = target.getAttribute('href') || '';
    var isOAuth = (text === 'Login with OAuth')
      || href.includes('github.com/login/oauth');
    if (!isOAuth) return;

    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();
    console.log('[oauth] intercepted OAuth click, opening popup');

    // If the button already has a resolved GitHub URL, use it directly
    if (href && href.includes('github.com/login/oauth')) {
      openOAuthPopup(href);
      return;
    }

    // Otherwise fetch from server
    var baseUrl = window.location.href.split('#')[0].split('?')[0].replace(/\/v1\/web\/.*/, '');
    fetch(baseUrl + '/oauth/authorize')
      .then(function (r) { return r.json(); })
      .then(function (data) {
        if (data.authorize_url) {
          openOAuthPopup(data.authorize_url);
        }
      })
      .catch(function (err) {
        console.warn('[oauth] failed to get authorize URL:', err);
      });
  }, true);

  // Check for just_logged_in flag BEFORE auth gate (auth gate may be disabled)
  var justLoggedInEarly = false;
  try {
    justLoggedInEarly = localStorage.getItem(oauthStorageKey('just_logged_in')) === 'true'
      || sessionStorage.getItem(oauthStorageKey('just_logged_in')) === 'true';
  } catch (_) {}
  if (justLoggedInEarly) {
    console.log('[oauth] fresh login detected (early check), will send success message');
    try {
      localStorage.removeItem(oauthStorageKey('just_logged_in'));
      sessionStorage.removeItem(oauthStorageKey('just_logged_in'));
    } catch (_) {}
    // Wait for webchat to fully initialize then send success message
    setTimeout(function () {
      sendOAuthSuccessToFlow();
    }, 3000);
  }

  // Run OAuth check immediately
  // First check for hash-based callback (server-side OAuth flow)
  if (!handleHashOAuthCallback()) {
    checkOAuthGate();
  }

})();

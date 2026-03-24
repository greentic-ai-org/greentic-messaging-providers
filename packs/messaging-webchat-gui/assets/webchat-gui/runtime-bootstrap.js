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

  var originalFetch = window.fetch.bind(window);
  window.fetch = function (input, init) {
    var requestUrl = typeof input === 'string' ? input : input.url;
    var url = new URL(requestUrl, window.location.href);
    console.log('[bootstrap] fetch:', url.pathname);

    if (/\/config\/tenants\/[^/]+\.json$/i.test(url.pathname)) {
      var tenantId = decodeURIComponent(url.pathname.split('/').pop().replace(/\.json$/i, ''));
      var locale = selectedLocale || 'en-US';
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
          providers: [
            {
              id: tenantId + '-demo',
              label: 'Demo Login',
              type: 'dummy',
              enabled: true
            }
          ]
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
        if (!response.ok) return response;
        var skinData = await response.json();
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
        console.log('[bootstrap] skin.json patched:', skinData.directLine, 'locale:', skinData.webchat?.locale);
        return new Response(JSON.stringify(skinData), {
          status: 200,
          headers: { 'Content-Type': 'application/json' }
        });
      });
    }

    return originalFetch(input, init);
  };

  // Build locale picker dynamically (DOMPurify strips <select>/<svg> from shell HTML)
  function initLocalePicker(mountEl) {
    var globeSvg = '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10A15.3 15.3 0 0 1 12 2z"/></svg>';

    var wrapper = document.createElement('label');
    wrapper.className = 'locale-picker';
    wrapper.innerHTML = globeSvg;

    var selectEl = document.createElement('select');
    selectEl.className = 'locale-picker__select';

    var codes = Object.keys(SUPPORTED_LOCALES).sort(function (a, b) {
      return SUPPORTED_LOCALES[a].localeCompare(SUPPORTED_LOCALES[b]);
    });
    var current = selectedLocale || 'en';
    codes.forEach(function (code) {
      var opt = document.createElement('option');
      opt.value = code;
      opt.textContent = SUPPORTED_LOCALES[code] + ' (' + code + ')';
      if (code === current) opt.selected = true;
      selectEl.appendChild(opt);
    });

    selectEl.addEventListener('change', function () {
      var params = new URLSearchParams(window.location.search);
      params.set('lang', this.value);
      window.location.search = params.toString();
    });

    wrapper.appendChild(selectEl);
    mountEl.appendChild(wrapper);
    console.log('[runtime-bootstrap] locale picker initialized, current:', current);
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

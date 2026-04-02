// Greentic WebChat Locale Picker Module
(function () {
  var rt = window.__GTC_RUNTIME__ || {};
  var tenant = rt.tenant || 'demo';
  var selectedLocale = rt.selectedLocale || '';
  var originalFetch = rt.originalFetch || window.fetch.bind(window);
  var uiT = rt.uiT || function(k, fb) { return fb || k; };
  var applyUiTranslations = rt.applyUiTranslations || function() {};

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

})();

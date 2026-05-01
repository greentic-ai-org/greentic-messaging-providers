// 3aigent skin hooks: theme persistence + toggle (page-reload approach).
// runtime-bootstrap reads localStorage["greentic.theme"] before fetching
// styleOptions, so reloading is sufficient to re-init Web Chat with the
// other theme's bubble palette.

(function () {
  'use strict';

  function readTheme() {
    try {
      var v = localStorage.getItem('greentic.theme');
      return v === 'light' ? 'light' : 'dark';
    } catch (_) {
      return 'dark';
    }
  }

  function applyTheme(theme) {
    document.documentElement.dataset.theme = theme;
    var btn = document.getElementById('theme-toggle');
    if (btn) {
      btn.textContent = theme === 'light' ? '☀️' : '🌙';
      btn.setAttribute('aria-pressed', theme === 'light' ? 'true' : 'false');
    }
  }

  // Apply on init (page-load)
  applyTheme(readTheme());

  // Re-apply on hooks.js late-load (icon may not have existed at first apply)
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function () { applyTheme(readTheme()); });
  } else {
    applyTheme(readTheme());
  }

  // Click handler — delegate so it works even if the button is rendered late
  document.addEventListener('click', function (ev) {
    var t = ev.target;
    if (!t || !t.closest) return;
    var btn = t.closest('#theme-toggle');
    if (!btn) return;
    var next = readTheme() === 'light' ? 'dark' : 'light';
    try { localStorage.setItem('greentic.theme', next); } catch (_) {}
    location.reload();
  });

  console.log('[hooks] 3aigent theme:', readTheme());
})();

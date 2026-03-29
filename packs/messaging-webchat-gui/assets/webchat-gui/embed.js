/**
 * Greentic WebChat Embed Script
 *
 * Adds a floating chat bubble to any website. Click to open a webchat window.
 * Default values (color, title, logo) are auto-loaded from the tenant's skin.json.
 * Explicit config in greenticChatConfig overrides skin.json defaults.
 *
 * Usage:
 *   <script>
 *     window.greenticChatConfig = {
 *       tenant: 'demo',                    // required — determines skin.json
 *       baseUrl: 'https://your-domain.com', // optional — auto-detected from script src
 *
 *       // All below are optional — defaults from skin.json
 *       bubble: {
 *         color: '#10B981',              // override brand.primary from skin
 *         hoverColor: '#059669',         // hover state color
 *         position: 'bottom-right',      // bottom-right | bottom-left
 *         size: 56,                      // button diameter (px)
 *         offset: 20,                    // distance from edge (px)
 *         offsetBottom: 20,              // distance from bottom (px)
 *         icon: null,                    // custom icon URL — replaces default chat icon
 *         iconSize: 28,                  // icon size inside bubble (px)
 *         label: 'Chat with us',         // tooltip text on hover
 *         borderRadius: '50%',           // '50%' circle, '12px' rounded square
 *       },
 *       window: {
 *         width: 400,                    // chat window width (px)
 *         height: 620,                   // chat window height (px)
 *         borderRadius: 12,              // corner radius (px)
 *         header: true,                  // show header bar
 *         headerColor: '#064e3b',        // header background
 *         headerTextColor: '#ffffff',     // header text color
 *         title: 'Greentic Assistant',   // header title — defaults to skin brand.name
 *         logo: null,                    // header logo URL — defaults to skin brand.logo
 *         logoSize: 24,                  // header logo size (px)
 *       },
 *       openOnLoad: false,               // auto-open on page load
 *       openDelay: 0,                    // delay (ms) before auto-open
 *       closeOnEscape: true,             // close with Escape key
 *       mobileFullscreen: true,          // fullscreen on mobile (<480px)
 *     };
 *   </script>
 *   <script src="https://your-domain.com/v1/web/webchat/demo/embed.js" defer></script>
 *
 * Public API (available after mount):
 *   greenticChat.open()    — open the chat window
 *   greenticChat.close()   — close the chat window
 *   greenticChat.toggle()  — toggle open/close
 *   greenticChat.isOpen()  — returns boolean
 */
(function () {
  'use strict';

  var config = window.greenticChatConfig || {};
  var tenant = config.tenant || 'demo';
  var baseUrl = (config.baseUrl || '').replace(/\/+$/, '');

  // Auto-detect baseUrl from script src if not provided
  if (!baseUrl) {
    var scripts = document.querySelectorAll('script[src*="embed.js"]');
    for (var i = 0; i < scripts.length; i++) {
      var src = scripts[i].src;
      var match = src.match(/^(https?:\/\/[^/]+)/);
      if (match) {
        baseUrl = match[1];
        break;
      }
    }
  }
  if (!baseUrl) {
    baseUrl = window.location.origin;
  }

  var webchatBase = baseUrl + '/v1/web/webchat/' + tenant;
  var chatUrl = webchatBase + '/';
  var skinUrl = webchatBase + '/skins/' + tenant + '/skin.json';

  // ── Fetch skin.json, then initialize ──
  fetch(skinUrl)
    .then(function (res) { return res.ok ? res.json() : {}; })
    .catch(function () { return {}; })
    .then(function (skin) { initialize(skin); });

  function initialize(skin) {
    var brand = (skin && skin.brand) || {};

    // ── Merge: explicit config > skin.json defaults > hardcoded defaults ──
    var bubble = config.bubble || {};
    var bubbleColor = bubble.color || brand.primary || '#10B981';
    var bubbleHoverColor = bubble.hoverColor || '';
    var bubblePosition = bubble.position || 'bottom-right';
    var bubbleSize = bubble.size || 56;
    var bubbleOffset = bubble.offset || 20;
    var bubbleOffsetBottom = bubble.offsetBottom || bubbleOffset;
    var bubbleIcon = bubble.icon || null;
    var bubbleIconSize = bubble.iconSize || 28;
    var bubbleLabel = bubble.label || '';
    var bubbleBorderRadius = bubble.borderRadius || '50%';

    var win = config.window || {};
    var winWidth = win.width || 400;
    var winHeight = win.height || 620;
    var winBorderRadius = win.borderRadius != null ? win.borderRadius : 12;
    var winHeader = win.header !== false;
    var winHeaderColor = win.headerColor || brand.primary || '#064e3b';
    var winHeaderTextColor = win.headerTextColor || '#ffffff';
    var winTitle = win.title || brand.name || 'Greentic Assistant';
    var winLogo = win.logo || (brand.logo ? webchatBase + brand.logo : null);
    var winLogoSize = win.logoSize || 24;
    var winShadow = win.shadow || '0 8px 40px rgba(0,0,0,0.2)';

    var openOnLoad = config.openOnLoad || false;
    var openDelay = config.openDelay || 0;
    var closeOnEscape = config.closeOnEscape !== false;
    var mobileFullscreen = config.mobileFullscreen !== false;

    var isOpen = false;
    var PREFIX = 'gtc-embed';
    var headerHeight = winHeader ? 44 : 0;

    // ── SVG Icons ──
    var CHAT_ICON =
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="white" width="' + bubbleIconSize + '" height="' + bubbleIconSize + '">' +
      '<path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2zm0 14H5.17L4 17.17V4h16v12z"/>' +
      '<path d="M7 9h10v2H7zm0-3h10v2H7zm0 6h7v2H7z"/>' +
      '</svg>';

    var CLOSE_ICON =
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="white" width="' + bubbleIconSize + '" height="' + bubbleIconSize + '">' +
      '<path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>' +
      '</svg>';

    function bubbleOpenContent() {
      if (bubbleIcon) {
        return '<img src="' + bubbleIcon + '" width="' + bubbleIconSize + '" height="' + bubbleIconSize + '" alt="Chat" style="border-radius:4px">';
      }
      return CHAT_ICON;
    }

    // ── Inject styles ──
    var isRight = bubblePosition === 'bottom-right';
    var posH = isRight ? 'right' : 'left';
    var posCSS = posH + ':' + bubbleOffset + 'px;';
    var winPosCSS = posH + ':' + bubbleOffset + 'px;';

    var hoverBg = bubbleHoverColor
      ? '#' + PREFIX + '-bubble:hover{background:' + bubbleHoverColor + '!important;}'
      : '';

    var style = document.createElement('style');
    style.textContent =
      '#' + PREFIX + '-bubble{' +
        'position:fixed;bottom:' + bubbleOffsetBottom + 'px;' + posCSS +
        'width:' + bubbleSize + 'px;height:' + bubbleSize + 'px;' +
        'border-radius:' + bubbleBorderRadius + ';background:' + bubbleColor + ';' +
        'border:none;cursor:pointer;z-index:2147483646;' +
        'display:flex;align-items:center;justify-content:center;' +
        'box-shadow:0 4px 16px rgba(0,0,0,0.18);' +
        'transition:transform 0.2s ease,box-shadow 0.2s ease,background 0.2s ease;' +
        'padding:0;outline:none;' +
      '}' +
      '#' + PREFIX + '-bubble:hover{' +
        'transform:scale(1.08);box-shadow:0 6px 24px rgba(0,0,0,0.22);' +
      '}' +
      hoverBg +
      '#' + PREFIX + '-label{' +
        'position:fixed;bottom:' + (bubbleOffsetBottom + bubbleSize + 8) + 'px;' + posCSS +
        'background:#1f2937;color:#fff;padding:6px 12px;border-radius:8px;' +
        'font-family:Inter,system-ui,sans-serif;font-size:13px;font-weight:500;' +
        'white-space:nowrap;pointer-events:none;opacity:0;' +
        'transition:opacity 0.2s ease;z-index:2147483646;' +
      '}' +
      '#' + PREFIX + '-bubble:hover~#' + PREFIX + '-label{opacity:1;}' +
      '#' + PREFIX + '-window{' +
        'position:fixed;bottom:' + (bubbleOffsetBottom + bubbleSize + 16) + 'px;' + winPosCSS +
        'width:' + winWidth + 'px;height:' + (winHeight + headerHeight) + 'px;' +
        'border:none;border-radius:' + winBorderRadius + 'px;z-index:2147483645;' +
        'box-shadow:' + winShadow + ';' +
        'overflow:hidden;' +
        'display:none;flex-direction:column;' +
        'background:#fff;' +
      '}' +
      '#' + PREFIX + '-window.open{display:flex;}' +
      '#' + PREFIX + '-header{' +
        'display:flex;align-items:center;gap:8px;' +
        'padding:0 12px;height:' + headerHeight + 'px;min-height:' + headerHeight + 'px;' +
        'background:' + winHeaderColor + ';color:' + winHeaderTextColor + ';' +
        'font-family:Inter,system-ui,sans-serif;font-size:14px;font-weight:600;' +
      '}' +
      '#' + PREFIX + '-header-logo{width:' + winLogoSize + 'px;height:' + winLogoSize + 'px;border-radius:4px;' +
        'filter:brightness(0) invert(1);}' +
      '#' + PREFIX + '-header-title{flex:1;}' +
      '#' + PREFIX + '-header-close{' +
        'background:none;border:none;cursor:pointer;padding:4px;' +
        'display:flex;align-items:center;justify-content:center;' +
        'border-radius:4px;transition:background 0.15s;color:inherit;' +
      '}' +
      '#' + PREFIX + '-header-close:hover{background:rgba(255,255,255,0.15);}' +
      '#' + PREFIX + '-iframe{' +
        'flex:1;width:100%;border:none;' +
      '}' +
      (mobileFullscreen ? (
        '@media(max-width:480px){' +
          '#' + PREFIX + '-window{' +
            'top:0!important;left:0!important;right:0!important;bottom:0!important;' +
            'width:100%!important;height:100%!important;border-radius:0!important;' +
          '}' +
        '}'
      ) : '');

    document.head.appendChild(style);

    // ── Create bubble button ──
    var btn = document.createElement('button');
    btn.id = PREFIX + '-bubble';
    btn.innerHTML = bubbleOpenContent();
    btn.setAttribute('aria-label', bubbleLabel || 'Open chat');

    // ── Create label tooltip ──
    var labelEl = document.createElement('div');
    labelEl.id = PREFIX + '-label';
    labelEl.textContent = bubbleLabel;

    // ── Create chat window ──
    var chatWindow = document.createElement('div');
    chatWindow.id = PREFIX + '-window';

    if (winHeader) {
      var header = document.createElement('div');
      header.id = PREFIX + '-header';
      if (winLogo) {
        var logoImg = document.createElement('img');
        logoImg.id = PREFIX + '-header-logo';
        logoImg.src = winLogo;
        logoImg.alt = '';
        header.appendChild(logoImg);
      }
      var dot = document.createElement('span');
      dot.style.cssText = 'width:8px;height:8px;border-radius:50%;background:#34d399;flex-shrink:0';
      header.appendChild(dot);
      var titleEl = document.createElement('span');
      titleEl.id = PREFIX + '-header-title';
      titleEl.textContent = winTitle;
      header.appendChild(titleEl);
      var closeBtn = document.createElement('button');
      closeBtn.id = PREFIX + '-header-close';
      closeBtn.innerHTML =
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" width="16" height="16">' +
        '<path d="M14.35 5.65a.5.5 0 00-.7 0L10 9.29 6.35 5.65a.5.5 0 00-.7.7L9.29 10l-3.64 3.65a.5.5 0 00.7.7L10 10.71l3.65 3.64a.5.5 0 00.7-.7L10.71 10l3.64-3.65a.5.5 0 000-.7z"/>' +
        '</svg>';
      closeBtn.setAttribute('aria-label', 'Close chat');
      closeBtn.addEventListener('click', function () { toggleChat(false); });
      header.appendChild(closeBtn);
      chatWindow.appendChild(header);
    }

    var iframe = document.createElement('iframe');
    iframe.id = PREFIX + '-iframe';
    iframe.allow = 'microphone; camera';
    iframe.title = winTitle;
    chatWindow.appendChild(iframe);

    // ── Toggle logic ──
    function toggleChat(forceState) {
      isOpen = forceState != null ? forceState : !isOpen;
      if (isOpen) {
        if (!iframe.src) {
          iframe.src = chatUrl;
        }
        chatWindow.classList.add('open');
        btn.innerHTML = CLOSE_ICON;
      } else {
        chatWindow.classList.remove('open');
        btn.innerHTML = bubbleOpenContent();
      }
    }

    btn.addEventListener('click', function () { toggleChat(); });

    if (closeOnEscape) {
      document.addEventListener('keydown', function (e) {
        if (e.key === 'Escape' && isOpen) {
          toggleChat(false);
        }
      });
    }

    // ── Mount ──
    document.body.appendChild(btn);
    if (bubbleLabel) {
      document.body.appendChild(labelEl);
    }
    document.body.appendChild(chatWindow);

    if (openOnLoad) {
      setTimeout(function () { toggleChat(true); }, openDelay);
    }

    // ── Public API ──
    window.greenticChat = {
      open: function () { toggleChat(true); },
      close: function () { toggleChat(false); },
      toggle: function () { toggleChat(); },
      isOpen: function () { return isOpen; },
    };
  }
})();

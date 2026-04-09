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
 *       tenant: 'default',                  // required — determines skin.json
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
 *         badge: true,                   // show unread message counter
 *         badgeColor: '#ef4444',         // badge background color
 *         greeting: '',                  // greeting text above bubble (e.g. 'Need help?')
 *         greetingDelay: 3000,           // ms before greeting appears (0 = immediate)
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
 *       fontFamily: 'Inter,system-ui,sans-serif', // font for all widget UI
 *       locale: '',                      // locale passed to iframe (e.g. 'id', 'en', 'ja')
 *       openOnLoad: false,               // auto-open on page load
 *       openDelay: 0,                    // delay (ms) before auto-open
 *       closeOnEscape: true,             // close with Escape key
 *       mobileFullscreen: true,          // fullscreen on mobile (<480px)
 *       persist: false,                  // persist open/close state across page navigation
 *       zIndex: 2147483646,              // z-index for bubble and window
 *       sound: false,                    // play notification sound on new bot message
 *       soundUrl: '',                    // custom notification sound URL (default: built-in beep)
 *       animation: true,                 // entrance animation for bubble on mount
 *
 *       // Event callbacks
 *       onOpen: null,                    // function() — called when chat opens
 *       onClose: null,                   // function() — called when chat closes
 *       onMessage: null,                 // function(data) — called on bot message
 *       onUserMessage: null,             // function(data) — called on user message
 *     };
 *   </script>
 *   <script src="https://your-domain.com/v1/web/webchat/demo/embed.js" defer></script>
 *
 * Public API (available after mount):
 *   greenticChat.open()       — open the chat window
 *   greenticChat.close()      — close the chat window
 *   greenticChat.toggle()     — toggle open/close
 *   greenticChat.isOpen()     — returns boolean
 *   greenticChat.hide()       — completely hide bubble + window
 *   greenticChat.show()       — show bubble again
 *   greenticChat.resetBadge() — clear unread counter
 */
(function () {
  'use strict';

  var config = window.greenticChatConfig || {};
  var tenant = config.tenant || 'default';
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

  var locale = config.locale || '';
  var webchatBase = baseUrl + '/v1/web/webchat/' + tenant;
  var chatUrl = webchatBase + '/' + (locale ? '?lang=' + encodeURIComponent(locale) : '');
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
    var bubbleBadge = bubble.badge !== false;
    var bubbleBadgeColor = bubble.badgeColor || '#ef4444';
    var bubbleGreeting = bubble.greeting || '';
    var bubbleGreetingDelay = bubble.greetingDelay != null ? bubble.greetingDelay : 3000;

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
    var persist = config.persist || false;
    var zIndex = config.zIndex || 2147483646;

    var fontFamily = config.fontFamily || 'Inter,system-ui,sans-serif';
    var enableSound = config.sound || false;
    var soundUrl = config.soundUrl || '';
    var enableAnimation = config.animation !== false;

    var onOpen = typeof config.onOpen === 'function' ? config.onOpen : null;
    var onClose = typeof config.onClose === 'function' ? config.onClose : null;
    var onMessage = typeof config.onMessage === 'function' ? config.onMessage : null;
    var onUserMessage = typeof config.onUserMessage === 'function' ? config.onUserMessage : null;

    var isOpen = false;
    var isHidden = false;
    var unreadCount = 0;
    var PREFIX = 'gtc-embed';
    var headerHeight = winHeader ? 44 : 0;
    var PERSIST_KEY = 'gtc-embed-state-' + tenant;

    // ── Sound notification ──
    var audioCtx = null;
    function playNotificationSound() {
      if (!enableSound || isOpen) return;
      if (soundUrl) {
        try { new Audio(soundUrl).play(); } catch (e) { /* ignore */ }
        return;
      }
      // Built-in beep using Web Audio API
      try {
        if (!audioCtx) audioCtx = new (window.AudioContext || window.webkitAudioContext)();
        var osc = audioCtx.createOscillator();
        var gain = audioCtx.createGain();
        osc.connect(gain);
        gain.connect(audioCtx.destination);
        osc.type = 'sine';
        osc.frequency.value = 880;
        gain.gain.value = 0.15;
        gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.3);
        osc.start(audioCtx.currentTime);
        osc.stop(audioCtx.currentTime + 0.3);
      } catch (e) { /* ignore */ }
    }

    // ── Restore persisted state ──
    if (persist) {
      try {
        var saved = sessionStorage.getItem(PERSIST_KEY);
        if (saved === 'open') { openOnLoad = true; openDelay = 0; }
      } catch (e) { /* ignore */ }
    }

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
        'border:none;cursor:pointer;z-index:' + zIndex + ';' +
        'display:flex;align-items:center;justify-content:center;' +
        'box-shadow:0 4px 16px rgba(0,0,0,0.18);' +
        'transition:transform 0.2s ease,box-shadow 0.2s ease,background 0.2s ease;' +
        'padding:0;outline:none;' +
      '}' +
      '#' + PREFIX + '-bubble:hover{' +
        'transform:scale(1.08);box-shadow:0 6px 24px rgba(0,0,0,0.22);' +
      '}' +
      hoverBg +
      '#' + PREFIX + '-badge{' +
        'position:absolute;top:-4px;' + (isRight ? 'right' : 'left') + ':-4px;' +
        'min-width:20px;height:20px;padding:0 6px;' +
        'background:' + bubbleBadgeColor + ';color:#fff;' +
        'font-family:' + fontFamily + ';font-size:11px;font-weight:700;' +
        'border-radius:10px;display:none;align-items:center;justify-content:center;' +
        'line-height:1;pointer-events:none;box-shadow:0 2px 6px rgba(0,0,0,0.2);' +
      '}' +
      '#' + PREFIX + '-badge.visible{display:flex;}' +
      '#' + PREFIX + '-label{' +
        'position:fixed;bottom:' + (bubbleOffsetBottom + bubbleSize + 8) + 'px;' + posCSS +
        'background:#1f2937;color:#fff;padding:6px 12px;border-radius:8px;' +
        'font-family:' + fontFamily + ';font-size:13px;font-weight:500;' +
        'white-space:nowrap;pointer-events:none;opacity:0;' +
        'transition:opacity 0.2s ease;z-index:' + zIndex + ';' +
      '}' +
      '#' + PREFIX + '-bubble:hover~#' + PREFIX + '-label{opacity:1;}' +
      '#' + PREFIX + '-greeting{' +
        'position:fixed;bottom:' + (bubbleOffsetBottom + bubbleSize + 12) + 'px;' + posCSS +
        'max-width:260px;background:#fff;color:#1f2937;' +
        'padding:12px 16px;border-radius:12px;' +
        'box-shadow:0 4px 20px rgba(0,0,0,0.12);border:1px solid #e5e7eb;' +
        'font-family:' + fontFamily + ';font-size:14px;line-height:1.4;' +
        'z-index:' + zIndex + ';cursor:pointer;' +
        'opacity:0;transform:translateY(8px);' +
        'transition:opacity 0.3s ease,transform 0.3s ease;' +
      '}' +
      '#' + PREFIX + '-greeting.visible{opacity:1;transform:translateY(0);}' +
      '#' + PREFIX + '-greeting-close{' +
        'position:absolute;top:4px;right:8px;background:none;border:none;' +
        'cursor:pointer;font-size:16px;color:#9ca3af;line-height:1;padding:2px;' +
      '}' +
      '#' + PREFIX + '-greeting-close:hover{color:#6b7280;}' +
      '#' + PREFIX + '-window{' +
        'position:fixed;bottom:' + (bubbleOffsetBottom + bubbleSize + 16) + 'px;' + winPosCSS +
        'width:' + winWidth + 'px;height:' + (winHeight + headerHeight) + 'px;' +
        'border:none;border-radius:' + winBorderRadius + 'px;z-index:' + (zIndex - 1) + ';' +
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
        'font-family:' + fontFamily + ';font-size:14px;font-weight:600;' +
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
      '.' + PREFIX + '-hidden{display:none!important;}' +
      '@keyframes ' + PREFIX + '-pop-in{' +
        '0%{transform:scale(0);opacity:0;}' +
        '70%{transform:scale(1.12);}' +
        '100%{transform:scale(1);opacity:1;}' +
      '}' +
      '#' + PREFIX + '-bubble.' + PREFIX + '-animate{' +
        'animation:' + PREFIX + '-pop-in 0.4s cubic-bezier(0.34,1.56,0.64,1) both;' +
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

    // ── Create badge ──
    var badgeEl = document.createElement('span');
    badgeEl.id = PREFIX + '-badge';
    badgeEl.textContent = '0';
    btn.appendChild(badgeEl);

    // ── Create label tooltip ──
    var labelEl = document.createElement('div');
    labelEl.id = PREFIX + '-label';
    labelEl.textContent = bubbleLabel;

    // ── Create greeting bubble ──
    var greetingEl = null;
    var greetingDismissed = false;
    if (bubbleGreeting) {
      greetingEl = document.createElement('div');
      greetingEl.id = PREFIX + '-greeting';
      greetingEl.innerHTML = '<span>' + bubbleGreeting + '</span>' +
        '<button id="' + PREFIX + '-greeting-close">&times;</button>';
    }

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

    // ── Badge logic ──
    function updateBadge() {
      if (!bubbleBadge) return;
      if (unreadCount > 0 && !isOpen) {
        badgeEl.textContent = unreadCount > 99 ? '99+' : String(unreadCount);
        badgeEl.classList.add('visible');
      } else {
        badgeEl.classList.remove('visible');
      }
    }

    function resetBadge() {
      unreadCount = 0;
      updateBadge();
    }

    // ── Greeting logic ──
    function showGreeting() {
      if (greetingEl && !greetingDismissed && !isOpen) {
        greetingEl.classList.add('visible');
      }
    }

    function dismissGreeting() {
      greetingDismissed = true;
      if (greetingEl) greetingEl.classList.remove('visible');
    }

    // ── iframe message listener (for unread badge + callbacks) ──
    window.addEventListener('message', function (event) {
      if (!event.data || typeof event.data !== 'object') return;
      var type = event.data.type || '';

      // Direct Line activity messages
      if (type === 'DIRECT_LINE/INCOMING_ACTIVITY' || type === 'gtc:botMessage') {
        if (!isOpen) {
          unreadCount++;
          updateBadge();
          playNotificationSound();
        }
        if (onMessage) {
          try { onMessage(event.data.payload || event.data); } catch (e) { /* ignore */ }
        }
      }

      if (type === 'WEB_CHAT/SEND_MESSAGE' || type === 'gtc:userMessage') {
        if (onUserMessage) {
          try { onUserMessage(event.data.payload || event.data); } catch (e) { /* ignore */ }
        }
      }
    });

    // ── Toggle logic ──
    function toggleChat(forceState) {
      isOpen = forceState != null ? forceState : !isOpen;
      if (isOpen) {
        if (!iframe.src) {
          iframe.src = chatUrl;
        }
        chatWindow.classList.add('open');
        btn.innerHTML = CLOSE_ICON;
        btn.appendChild(badgeEl);
        resetBadge();
        dismissGreeting();
        if (onOpen) { try { onOpen(); } catch (e) { /* ignore */ } }
      } else {
        chatWindow.classList.remove('open');
        btn.innerHTML = bubbleOpenContent();
        btn.appendChild(badgeEl);
        if (onClose) { try { onClose(); } catch (e) { /* ignore */ } }
      }

      if (persist) {
        try { sessionStorage.setItem(PERSIST_KEY, isOpen ? 'open' : 'closed'); } catch (e) { /* ignore */ }
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
    if (enableAnimation) btn.classList.add(PREFIX + '-animate');
    document.body.appendChild(btn);
    if (bubbleLabel) {
      document.body.appendChild(labelEl);
    }
    if (greetingEl) {
      document.body.appendChild(greetingEl);
      greetingEl.querySelector('#' + PREFIX + '-greeting-close').addEventListener('click', function (e) {
        e.stopPropagation();
        dismissGreeting();
      });
      greetingEl.addEventListener('click', function () {
        dismissGreeting();
        toggleChat(true);
      });
      if (bubbleGreetingDelay > 0) {
        setTimeout(showGreeting, bubbleGreetingDelay);
      } else {
        showGreeting();
      }
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
      hide: function () {
        isHidden = true;
        btn.classList.add(PREFIX + '-hidden');
        chatWindow.classList.add(PREFIX + '-hidden');
        if (labelEl) labelEl.classList.add(PREFIX + '-hidden');
        if (greetingEl) greetingEl.classList.add(PREFIX + '-hidden');
      },
      show: function () {
        isHidden = false;
        btn.classList.remove(PREFIX + '-hidden');
        chatWindow.classList.remove(PREFIX + '-hidden');
        if (labelEl) labelEl.classList.remove(PREFIX + '-hidden');
        if (greetingEl) greetingEl.classList.remove(PREFIX + '-hidden');
      },
      resetBadge: resetBadge,
    };
  }
})();

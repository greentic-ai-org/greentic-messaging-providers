// Store reference for dispatching messages from quick buttons
var __webchatStore = null;

export function createStoreMiddleware() {
  return (store) => {
    __webchatStore = store;
    return next => action => {
    const result = next(action);

    // Auto-scroll on bot messages
    if (
      action.type === 'DIRECT_LINE/INCOMING_ACTIVITY' &&
      action.payload?.activity?.from?.role === 'bot'
    ) {
      setTimeout(() => {
        window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
      }, 200);
    }

    return result;
  };};
}

export function onBeforeRender(context) {
  console.info('[hooks] rendering tenant', context.skin.tenant);
  injectEmbedCardStyles();
  setTimeout(injectMicButton, 500);
}

// ── Adaptive Card fit-up for the embed surface ────────────

// The SPA gives the embed container `embed-webchat-surface tenant-widget-surface`
// but not `widget-surface`, so the stylesheet's card rules never match and long
// action labels clip. Durable fix is one word in the SPA; this covers shipped builds.
var embedStylesInjected = false;

function injectEmbedCardStyles() {
  if (embedStylesInjected) return;
  embedStylesInjected = true;

  var css = [
    '.embed-webchat-surface .webchat__bubble,',
    '.embed-webchat-surface .webchat__bubble__content,',
    '.embed-webchat-surface .webchat__stacked-layout__message{max-width:100%}',
    '.embed-webchat-surface .ac-adaptiveCard,',
    '.embed-webchat-surface .ac-container,',
    '.embed-webchat-surface .ac-columnSet,',
    '.embed-webchat-surface .ac-textBlock{max-width:100%!important}',
    '.embed-webchat-surface .ac-adaptiveCard{width:100%!important;box-sizing:border-box}',
    '.embed-webchat-surface .ac-actionSet,',
    ".embed-webchat-surface .ac-horizontal-separator+div[role='group']",
    '{display:flex!important;flex-wrap:wrap!important;gap:.5rem;width:100%}',
    '.embed-webchat-surface .ac-actionSet>button,',
    '.embed-webchat-surface .ac-actionSet .ac-pushButton,',
    ".embed-webchat-surface .ac-horizontal-separator+div[role='group']>button",
    '{flex:1 1 140px;max-width:100%;min-width:0;min-height:40px;',
    'white-space:normal!important;overflow-wrap:anywhere}'
  ].join('');

  var style = document.createElement('style');
  style.id = 'greentic-embed-card-fit';
  style.textContent = css;
  document.head.appendChild(style);
}

// ── Voice input (Web Speech API) ──────────────────────────

var micInjected = false;
var recognition = null;
var isListening = false;

function injectMicButton() {
  if (micInjected) return;
  if (!('webkitSpeechRecognition' in window) && !('SpeechRecognition' in window)) return;

  // Find the send box area
  var sendBox = document.querySelector('[class*="send-box"]') ||
    document.querySelector('[class*="sendbox"]') ||
    document.querySelector('form[class*="send"]');
  if (!sendBox) {
    // Retry — webchat might not be fully rendered
    setTimeout(injectMicButton, 1000);
    return;
  }

  micInjected = true;

  var btn = document.createElement('button');
  btn.id = 'greentic-mic-btn';
  btn.type = 'button';
  btn.title = 'Voice input';
  btn.setAttribute('aria-label', 'Voice input');
  btn.style.cssText = [
    'background:none;border:none;cursor:pointer;padding:6px;',
    'display:flex;align-items:center;justify-content:center;',
    'color:#0b7f5b;transition:all .15s;border-radius:50%;',
    'width:36px;height:36px;flex-shrink:0;'
  ].join('');
  // Static local SVG markup; no user-controlled input reaches this assignment.
  // foxguard: ignore[js/no-xss-innerhtml]
  btn.innerHTML = micSvg();
  btn.onclick = toggleMic;

  // Insert before send button or at end of send box
  var sendBtn = sendBox.querySelector('button[title="Send"]') ||
    sendBox.querySelector('button[type="submit"]') ||
    sendBox.querySelector('[class*="send-button"]');
  if (sendBtn && sendBtn.parentElement) {
    sendBtn.parentElement.insertBefore(btn, sendBtn);
  } else {
    sendBox.appendChild(btn);
  }
}

function micSvg() {
  return '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>';
}

function micListeningSvg() {
  return '<svg width="20" height="20" viewBox="0 0 24 24" fill="#ef4444" stroke="#ef4444" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>';
}

function toggleMic() {
  if (isListening) {
    stopMic();
  } else {
    startMic();
  }
}

function startMic() {
  var SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SpeechRecognition) return;

  recognition = new SpeechRecognition();
  recognition.continuous = false;
  recognition.interimResults = true;
  recognition.lang = document.documentElement.lang || 'en-US';

  var btn = document.getElementById('greentic-mic-btn');

  recognition.onstart = function () {
    isListening = true;
    if (btn) {
      // Static local SVG markup; no user-controlled input reaches this assignment.
      // foxguard: ignore[js/no-xss-innerhtml]
      btn.innerHTML = micListeningSvg();
      btn.style.background = 'rgba(239,68,68,0.1)';
    }
  };

  recognition.onresult = function (event) {
    var transcript = '';
    for (var i = event.resultIndex; i < event.results.length; i++) {
      transcript += event.results[i][0].transcript;
    }
    // Put transcript into send box
    var sendBox = document.querySelector('[data-id="webchat-sendbox-input"]') ||
      document.querySelector('input[placeholder]') ||
      document.querySelector('textarea');
    if (sendBox) {
      var setter = Object.getOwnPropertyDescriptor(
        window.HTMLInputElement.prototype, 'value'
      ).set || Object.getOwnPropertyDescriptor(
        window.HTMLTextAreaElement.prototype, 'value'
      ).set;
      setter.call(sendBox, transcript);
      sendBox.dispatchEvent(new Event('input', { bubbles: true }));

      // Auto-submit on final result
      if (event.results[event.resultIndex].isFinal) {
        setTimeout(function () {
          var sendBtn = document.querySelector('[title="Send"]') ||
            document.querySelector('button[type="submit"]');
          if (sendBtn) sendBtn.click();
        }, 200);
      }
    }
  };

  recognition.onerror = function (event) {
    console.warn('[voice] error:', event.error);
    stopMic();
  };

  recognition.onend = function () {
    stopMic();
  };

  recognition.start();
}

function stopMic() {
  isListening = false;
  if (recognition) {
    try { recognition.stop(); } catch (_) {}
    recognition = null;
  }
  var btn = document.getElementById('greentic-mic-btn');
  if (btn) {
    // Static local SVG markup; no user-controlled input reaches this assignment.
    // foxguard: ignore[js/no-xss-innerhtml]
    btn.innerHTML = micSvg();
    btn.style.background = 'none';
  }
}

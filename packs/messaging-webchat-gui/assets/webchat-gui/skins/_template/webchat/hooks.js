// Store reference for dispatching messages from quick buttons
var __webchatStore = null;

export function createStoreMiddleware() {
  var welcomeShown = false;
  var hasUserMessage = false;

  return (store) => {
    __webchatStore = store;
    return next => action => {
    // Track user messages to hide welcome
    if (action.type === 'WEB_CHAT/SEND_MESSAGE' || action.type === 'WEB_CHAT/SEND_EVENT') {
      hasUserMessage = true;
      hideWelcome();
    }

    const result = next(action);

    // Auto-scroll on bot messages
    if (
      action.type === 'DIRECT_LINE/INCOMING_ACTIVITY' &&
      action.payload?.activity?.from?.role === 'bot'
    ) {
      hasUserMessage = true;
      hideWelcome();
      setTimeout(() => {
        window.scrollTo({ top: document.body.scrollHeight, behavior: 'smooth' });
      }, 200);
    }

    // Show welcome on first connect if no messages yet
    if (action.type === 'DIRECT_LINE/CONNECT_FULFILLED' && !welcomeShown && !hasUserMessage) {
      welcomeShown = true;
      setTimeout(showWelcome, 300);
    }

    return result;
  };};
}

function showWelcome() {
  var webchat = document.getElementById('webchat');
  if (!webchat) return;

  // Check if messages already exist
  var existing = webchat.querySelector('[class*="activity"]');
  if (existing) return;

  var welcome = document.createElement('div');
  welcome.id = 'greentic-welcome';
  welcome.className = 'welcome';

  welcome.innerHTML = [
    '<div class="welcome__card">',
      '<div class="welcome__icon">',
        '<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#059669" ',
          'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">',
          '<path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/>',
        '</svg>',
      '</div>',
      '<h2 class="welcome__heading">Welcome! 👋</h2>',
      '<p class="welcome__text">',
        'How can I help you today?',
      '</p>',
      '<div class="welcome__actions">',
        '<button class="welcome__btn" onclick="sendQuickMessage(this)">🚀 Get started</button>',
        '<button class="welcome__btn" onclick="sendQuickMessage(this)">❓ What can you do?</button>',
      '</div>',
    '</div>'
  ].join('');

  // Insert before webchat content
  var surface = webchat.querySelector('[role="main"]') || webchat.firstElementChild || webchat;
  surface.style.position = 'relative';
  surface.appendChild(welcome);
}

function hideWelcome() {
  var el = document.getElementById('greentic-welcome');
  if (el) {
    el.style.opacity = '0';
    el.style.transition = 'opacity 0.3s ease';
    setTimeout(function () { el.remove(); }, 300);
  }
}

// Global function for quick message buttons — dispatch via WebChat store
window.sendQuickMessage = function (btn) {
  var text = btn.textContent.replace(/^[^\w\s]+\s*/, '').trim();
  if (__webchatStore && text) {
    __webchatStore.dispatch({ type: 'WEB_CHAT/SEND_MESSAGE', payload: { text: text } });
  }
  hideWelcome();
};

export function onBeforeRender(context) {
  console.info('[hooks] rendering tenant', context.skin.tenant);
  setTimeout(injectMicButton, 500);
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
    'color:#059669;transition:all .15s;border-radius:50%;',
    'width:36px;height:36px;flex-shrink:0;'
  ].join('');
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
    btn.innerHTML = micSvg();
    btn.style.background = 'none';
  }
}

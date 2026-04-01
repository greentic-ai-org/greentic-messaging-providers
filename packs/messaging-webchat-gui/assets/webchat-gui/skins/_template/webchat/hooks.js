export function createStoreMiddleware() {
  var welcomeShown = false;
  var hasUserMessage = false;

  return () => next => action => {
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
  };
}

function showWelcome() {
  var webchat = document.getElementById('webchat');
  if (!webchat) return;

  // Check if messages already exist
  var existing = webchat.querySelector('[class*="activity"]');
  if (existing) return;

  var welcome = document.createElement('div');
  welcome.id = 'greentic-welcome';
  welcome.style.cssText = [
    'position:absolute;inset:0;display:flex;flex-direction:column;',
    'align-items:center;justify-content:center;padding:2rem;',
    'pointer-events:none;z-index:1;',
    'font-family:Poppins,system-ui,sans-serif;'
  ].join('');

  welcome.innerHTML = [
    '<div style="pointer-events:auto;text-align:center;max-width:400px;">',
      '<div style="width:64px;height:64px;border-radius:50%;background:#ecfdf5;',
        'display:flex;align-items:center;justify-content:center;margin:0 auto 1.25rem;">',
        '<svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#059669" ',
          'stroke-width="2" stroke-linecap="round" stroke-linejoin="round">',
          '<path d="M20 2H4c-1.1 0-2 .9-2 2v18l4-4h14c1.1 0 2-.9 2-2V4c0-1.1-.9-2-2-2z"/>',
        '</svg>',
      '</div>',
      '<h2 style="margin:0 0 0.5rem;font-size:1.25rem;font-weight:600;color:#1f2937;">',
        'Hi there! 👋',
      '</h2>',
      '<p style="margin:0 0 1.5rem;font-size:0.875rem;color:#6b7280;line-height:1.6;">',
        'I\'m your AI assistant. I can help you with onboarding, answer questions about your setup, ',
        'and guide you through common tasks.',
      '</p>',
      '<div style="display:flex;flex-wrap:wrap;gap:0.5rem;justify-content:center;">',
        '<button onclick="sendQuickMessage(this)" style="padding:0.5rem 1rem;border:1px solid #d1fae5;',
          'border-radius:20px;background:#ecfdf5;color:#059669;font-size:0.8125rem;font-weight:500;',
          'cursor:pointer;font-family:inherit;transition:all .15s;"',
          'onmouseover="this.style.background=\'#d1fae5\'" onmouseout="this.style.background=\'#ecfdf5\'"',
          '>🚀 Get started</button>',
        '<button onclick="sendQuickMessage(this)" style="padding:0.5rem 1rem;border:1px solid #d1fae5;',
          'border-radius:20px;background:#ecfdf5;color:#059669;font-size:0.8125rem;font-weight:500;',
          'cursor:pointer;font-family:inherit;transition:all .15s;"',
          'onmouseover="this.style.background=\'#d1fae5\'" onmouseout="this.style.background=\'#ecfdf5\'"',
          '>❓ What can you do?</button>',
        '<button onclick="sendQuickMessage(this)" style="padding:0.5rem 1rem;border:1px solid #d1fae5;',
          'border-radius:20px;background:#ecfdf5;color:#059669;font-size:0.8125rem;font-weight:500;',
          'cursor:pointer;font-family:inherit;transition:all .15s;"',
          'onmouseover="this.style.background=\'#d1fae5\'" onmouseout="this.style.background=\'#ecfdf5\'"',
          '>📖 Show me a demo</button>',
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

// Global function for quick message buttons
window.sendQuickMessage = function (btn) {
  var text = btn.textContent.replace(/^[^\w\s]+\s*/, '').trim();
  var sendBox = document.querySelector('[data-id="webchat-sendbox-input"]') ||
    document.querySelector('input[placeholder]') ||
    document.querySelector('textarea');
  if (sendBox) {
    var nativeInputValueSetter = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype, 'value'
    ).set || Object.getOwnPropertyDescriptor(
      window.HTMLTextAreaElement.prototype, 'value'
    ).set;
    nativeInputValueSetter.call(sendBox, text);
    sendBox.dispatchEvent(new Event('input', { bubbles: true }));
    setTimeout(function () {
      var sendBtn = document.querySelector('[title="Send"]') ||
        document.querySelector('button[type="submit"]');
      if (sendBtn) sendBtn.click();
    }, 100);
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

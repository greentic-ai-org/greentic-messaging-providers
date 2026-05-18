(function () {
  function createElement(tag, attrs) {
    var element = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) {
      if (key === 'text') element.textContent = attrs[key];
      else element.setAttribute(key, attrs[key]);
    });
    return element;
  }

  function appendMessage(transcript, from, text) {
    var message = createElement('div', {
      class: 'webchat-test-message webchat-test-message--' + from,
      'data-testid': 'webchat-message',
    });
    message.textContent = text;
    transcript.appendChild(message);
    transcript.scrollTop = transcript.scrollHeight;
  }

  function appendAdaptiveCard(transcript) {
    var bubble = createElement('div', {
      class: 'webchat__bubble webchat-test-message webchat-test-message--bot',
      'data-testid': 'webchat-adaptive-card-message',
    });
    var content = createElement('div', {
      class: 'webchat__bubble__content',
    });
    var card = createElement('div', {
      class: 'ac-adaptiveCard',
      'data-testid': 'adaptive-card',
    });
    card.style.cssText = 'display:block;padding:16px;border:1px solid #d1d5db;background:#fff;';
    var title = createElement('div', {
      class: 'ac-textBlock',
      'data-testid': 'adaptive-card-title',
      text: 'Adaptive card preview',
    });
    title.style.cssText = 'color:#f5f5f5;';
    card.appendChild(title);
    var actionSet = createElement('div', {
      class: 'ac-actionSet',
      role: 'group',
    });
    var action = createElement('button', {
      'data-testid': 'adaptive-card-action',
      type: 'button',
      text: 'Adaptive Cards docs',
    });
    action.addEventListener('click', function () {
      action.disabled = true;
      action.setAttribute('aria-busy', 'true');
    });
    actionSet.appendChild(action);
    card.appendChild(actionSet);
    content.appendChild(card);
    bubble.appendChild(content);
    transcript.appendChild(bubble);
    transcript.scrollTop = transcript.scrollHeight;
  }

  function renderWebChat(_config, element) {
    element.innerHTML = '';
    element.setAttribute('data-testid', 'webchat-surface');

    var root = createElement('section', {
      class: 'webchat-test-root',
      'aria-label': 'Greentic WebChat conversation',
      'data-testid': 'webchat-root',
    });
    var transcript = createElement('div', {
      class: 'webchat-test-transcript',
      role: 'log',
      'aria-live': 'polite',
      'data-testid': 'webchat-transcript',
    });
    var form = createElement('form', {
      class: 'webchat-test-form',
      'data-testid': 'webchat-form',
    });
    var label = createElement('label', {
      class: 'webchat-test-label',
      for: 'webchat-test-input-' + Math.random().toString(36).slice(2),
      text: 'Message',
    });
    var input = createElement('input', {
      id: label.getAttribute('for'),
      class: 'webchat-test-input',
      'data-testid': 'webchat-input',
      'aria-label': 'Type your message',
      placeholder: 'Type your message',
      autocomplete: 'off',
    });
    var button = createElement('button', {
      type: 'submit',
      class: 'webchat-test-send',
      'data-testid': 'webchat-send',
      text: 'Send',
    });

    form.append(label, input, button);
    root.append(transcript, form);
    element.appendChild(root);
    appendMessage(transcript, 'bot', 'Hello from Greentic');
    appendAdaptiveCard(transcript);

    form.addEventListener('submit', function (event) {
      event.preventDefault();
      var value = input.value.trim();
      if (!value) return;
      appendMessage(transcript, 'user', value);
      input.value = '';
      window.fetch('/mock-api/messages', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ text: value }),
      })
        .then(function (response) { return response.json(); })
        .then(function (payload) {
          appendMessage(transcript, 'bot', payload.text || 'Hello from Greentic');
        })
        .catch(function () {
          appendMessage(transcript, 'bot', 'Hello from Greentic');
        });
    });
  }

  window.WebChat = {
    createDirectLine: function (options) {
      return {
        token: options && options.token,
        domain: options && options.domain,
        postActivity: function () {
          return { subscribe: function (next) { if (next) next('mock-activity'); } };
        },
      };
    },
    createStore: function () {
      return {
        dispatch: function () {},
        getState: function () { return {}; },
        subscribe: function () { return function () {}; },
      };
    },
    renderWebChat: renderWebChat,
  };
})();

const template = document.createElement("template");

template.innerHTML = `
  <style>
    :host {
      --greentic-webchat-z: 2147483646;
      --greentic-webchat-accent: #10b981;
      --greentic-webchat-accent-hover: #059669;
      --greentic-webchat-radius: 12px;
      color-scheme: light;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    .frame {
      width: min(100%, 420px);
      height: min(680px, 80vh);
      border: 0;
      border-radius: var(--greentic-webchat-radius);
      box-shadow: 0 18px 50px rgba(15, 23, 42, 0.24);
      background: #fff;
      overflow: hidden;
    }

    .dock {
      position: fixed;
      right: 20px;
      bottom: 92px;
      z-index: var(--greentic-webchat-z);
      display: none;
    }

    .dock[data-open="true"] {
      display: block;
    }

    .inline {
      display: block;
      width: 100%;
      min-height: 520px;
    }

    .inline .frame {
      width: 100%;
      height: 100%;
      min-height: inherit;
      box-shadow: none;
    }

    button.launcher {
      position: fixed;
      right: 20px;
      bottom: 20px;
      z-index: var(--greentic-webchat-z);
      width: 56px;
      height: 56px;
      border: 0;
      border-radius: 50%;
      display: inline-grid;
      place-items: center;
      color: #fff;
      background: var(--greentic-webchat-accent);
      box-shadow: 0 12px 30px rgba(15, 23, 42, 0.28);
      cursor: pointer;
    }

    button.launcher:hover {
      background: var(--greentic-webchat-accent-hover);
    }

    button.launcher:focus-visible {
      outline: 3px solid rgba(16, 185, 129, 0.35);
      outline-offset: 3px;
    }

    .icon {
      width: 28px;
      height: 28px;
      fill: currentColor;
    }

    @media (max-width: 520px) {
      .dock {
        inset: 0;
        bottom: 0;
        right: 0;
      }

      .dock .frame {
        width: 100vw;
        height: 100vh;
        border-radius: 0;
      }
    }
  </style>
  <div class="inline" part="inline" hidden></div>
  <div class="dock" part="dock"></div>
  <button class="launcher" part="launcher" type="button" aria-expanded="false">
    <svg class="icon" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 4h16v12H7.4L4 19.4V4Zm2 2v8.6l.6-.6H18V6H6Zm2 3h8v1.8H8V9Zm0 3h5v1.8H8V12Z"/>
    </svg>
  </button>
`;

function boolAttr(value, fallback = false) {
  if (value == null) return fallback;
  return value === "" || value === "true" || value === "1";
}

function scriptPublicBaseUrl() {
  const current = import.meta.url || (document.currentScript && document.currentScript.src);
  if (!current) return window.location.origin;
  try {
    const url = new URL(current);
    const marker = "/v1/web/webchat/";
    const index = url.pathname.indexOf(marker);
    if (index >= 0) {
      return `${url.origin}${url.pathname.slice(0, index)}`;
    }
    return url.origin;
  } catch {
    return window.location.origin;
  }
}

class GreenticWebchatElement extends HTMLElement {
  static get observedAttributes() {
    return [
      "tenant",
      "api-base",
      "public-base-url",
      "skin",
      "launcher",
      "open",
      "locale",
      "title",
    ];
  }

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this.shadowRoot.append(template.content.cloneNode(true));
    this._dock = this.shadowRoot.querySelector(".dock");
    this._inline = this.shadowRoot.querySelector(".inline");
    this._launcher = this.shadowRoot.querySelector(".launcher");
    this._iframe = null;
    this._ready = false;
    this._launcher.addEventListener("click", () => this.toggle());
  }

  connectedCallback() {
    this.render();
    queueMicrotask(() => {
      if (!this._ready) {
        this._ready = true;
        this.dispatch("greentic-webchat-ready");
      }
    });
  }

  attributeChangedCallback() {
    if (this.isConnected) this.render();
  }

  get tenant() {
    return this.getAttribute("tenant") || "default";
  }

  set tenant(value) {
    this.setAttribute("tenant", value);
  }

  get open() {
    return this.hasAttribute("open");
  }

  set open(value) {
    if (value) this.setAttribute("open", "");
    else this.removeAttribute("open");
  }

  get launcher() {
    return boolAttr(this.getAttribute("launcher"), true);
  }

  set launcher(value) {
    if (value) this.setAttribute("launcher", "true");
    else this.setAttribute("launcher", "false");
  }

  show() {
    this.hidden = false;
  }

  hide() {
    this.hidden = true;
  }

  toggle() {
    this.open ? this.close() : this.openChat();
  }

  openChat() {
    if (!this.open) {
      this.open = true;
      this.dispatch("greentic-webchat-open");
    }
  }

  close() {
    if (this.open) {
      this.open = false;
      this.dispatch("greentic-webchat-close");
    }
  }

  dispatch(name, detail = {}) {
    this.dispatchEvent(new CustomEvent(name, { bubbles: true, composed: true, detail }));
  }

  render() {
    try {
      const useLauncher = this.launcher;
      this._launcher.hidden = !useLauncher;
      this._launcher.setAttribute("aria-expanded", String(this.open));
      this._launcher.setAttribute("aria-label", this.getAttribute("title") || "Open chat");

      const target = useLauncher ? this._dock : this._inline;
      this._inline.hidden = useLauncher;
      this._dock.dataset.open = String(useLauncher && this.open);

      if (!this._iframe || this._iframe.parentElement !== target) {
        this._iframe && this._iframe.remove();
        this._iframe = document.createElement("iframe");
        this._iframe.className = "frame";
        this._iframe.setAttribute("part", "iframe");
        this._iframe.setAttribute("allow", "clipboard-write");
        target.append(this._iframe);
      }

      this._iframe.title = this.getAttribute("title") || "Greentic WebChat";
      this._iframe.src = this.webchatUrl();
    } catch (error) {
      this.dispatch("greentic-webchat-error", { message: String(error && error.message || error) });
    }
  }

  webchatUrl() {
    const publicBase = (this.getAttribute("public-base-url") || scriptPublicBaseUrl()).replace(/\/+$/, "");
    const tenant = encodeURIComponent(this.tenant);
    const url = new URL(`${publicBase}/v1/web/webchat/${tenant}/`);
    const apiBase = this.getAttribute("api-base");
    const skin = this.getAttribute("skin");
    const locale = this.getAttribute("locale");
    if (apiBase) url.searchParams.set("apiBase", apiBase);
    if (skin) url.searchParams.set("skin", skin);
    if (locale) url.searchParams.set("lang", locale);
    url.searchParams.set("presentation_mode", "embed_webcomponent");
    return url.toString();
  }
}

if (!customElements.get("greentic-webchat")) {
  customElements.define("greentic-webchat", GreenticWebchatElement);
}

const legacyConfig = window.greenticChatConfig;
if (legacyConfig && !document.querySelector("greentic-webchat[data-greentic-legacy]")) {
  const element = document.createElement("greentic-webchat");
  element.dataset.greenticLegacy = "true";
  if (legacyConfig.tenant) element.setAttribute("tenant", legacyConfig.tenant);
  if (legacyConfig.baseUrl) element.setAttribute("public-base-url", legacyConfig.baseUrl);
  if (legacyConfig.apiBase) element.setAttribute("api-base", legacyConfig.apiBase);
  if (legacyConfig.skin) element.setAttribute("skin", legacyConfig.skin);
  if (legacyConfig.locale) element.setAttribute("locale", legacyConfig.locale);
  if (legacyConfig.openOnLoad) element.setAttribute("open", "");
  const appendLegacyElement = () => document.body.append(element);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", appendLegacyElement, { once: true });
  } else {
    appendLegacyElement();
  }
  window.greenticChat = {
    open: () => element.openChat(),
    close: () => element.close(),
    toggle: () => element.toggle(),
    isOpen: () => element.open,
    hide: () => element.hide(),
    show: () => element.show(),
  };
}

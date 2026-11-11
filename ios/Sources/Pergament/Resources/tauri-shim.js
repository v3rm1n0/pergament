// Stand-in for the bridge Tauri injects on desktop (window.__TAURI_INTERNALS__).
// Commands go to the Rust core through the `pergament` script message handler
// (Core.swift), which answers with window.__pergament_reply and pushes core
// events through window.__pergament_event. Event subscriptions live here, and
// plugin commands without an iOS stand-in fail with a short message that the
// UI shows as a toast.
(() => {
  if (window.__TAURI_INTERNALS__) return;

  // Tauri rejects with the command's Err string, and the UI toasts String(e); an Error would show "Error: ...".
  const unavailable = "not available on iOS yet";
  const label = "main";
  let nextId = 1;
  // Seeded from the load time so a late reply to a call from before a reload cannot match a new call; stays below 2^53.
  let nextCall = Date.now() * 1000;
  const pending = new Map();
  // eventId -> { event, handler }, handler being a transformCallback id.
  const listeners = new Map();

  window.__PERGAMENT_PLATFORM__ = "ios";

  function deliver(event, payload) {
    for (const [id, listener] of listeners) {
      if (listener.event === event) window[`_${listener.handler}`]?.({ event, id, payload });
    }
  }

  // Only the targets that match this one webview; the desktop's other windows do not exist here.
  function targetsThisWebview(target) {
    return !target || target.kind === "Any" || target.label === label;
  }

  function callNative(name, args) {
    const native = window.webkit?.messageHandlers?.pergament;
    if (!native) return Promise.reject(unavailable);
    const id = nextCall++;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      try {
        native.postMessage({ id, name, args: JSON.stringify(args ?? null) });
      } catch (e) {
        pending.delete(id);
        reject(String(e));
      }
    });
  }

  // Desktop answers `media` in its Tauri shell; here the page fetches the URL from the jwmedia
  // scheme handler (MediaSchemeHandler.swift) and the bytes never cross the JSON bridge.
  const mediaPrefix = "jwmedia://localhost";

  async function media(url) {
    if (typeof url !== "string" || !url.startsWith(mediaPrefix)) throw "not a media URL";
    const missing = `no image at ${url.slice(mediaPrefix.length)}`;
    let response;
    try {
      response = await fetch(url);
    } catch {
      throw missing;
    }
    if (!response.ok) throw missing;
    return response.arrayBuffer();
  }

  window.__pergament_reply = (id, ok, json) => {
    const call = pending.get(id);
    if (!call) return;
    pending.delete(id);
    let value;
    try {
      value = JSON.parse(json);
    } catch (e) {
      call.reject(String(e));
      return;
    }
    if (ok) call.resolve(value);
    else call.reject(value);
  };

  window.__pergament_event = (event, json) => deliver(event, JSON.parse(json));

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, eventId) {
      const listener = listeners.get(eventId);
      if (!listener) return;
      listeners.delete(eventId);
      window.__TAURI_INTERNALS__.unregisterCallback(listener.handler);
    },
  };

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label },
      currentWebview: { label, windowLabel: label },
      windows: [{ label }],
      webviews: [{ label, windowLabel: label }],
    },

    transformCallback(callback, once) {
      const id = nextId++;
      const name = `_${id}`;
      Object.defineProperty(window, name, {
        value: (result) => {
          if (once) Reflect.deleteProperty(window, name);
          return callback?.(result);
        },
        writable: false,
        configurable: true,
      });
      return id;
    },

    unregisterCallback(id) {
      Reflect.deleteProperty(window, `_${id}`);
    },

    // MediaSchemeHandler serves asset:// from the media folder; the core decodes the encoded path.
    convertFileSrc(path, protocol = "asset") {
      return `${protocol}://localhost/${encodeURIComponent(path)}`;
    },

    invoke(cmd, args) {
      switch (cmd) {
        case "plugin:event|listen": {
          const eventId = nextId++;
          listeners.set(eventId, { event: args?.event, handler: args?.handler });
          return Promise.resolve(eventId);
        }
        case "plugin:event|unlisten":
          listeners.delete(args?.eventId);
          return Promise.resolve();
        case "plugin:event|emit":
          deliver(args?.event, args?.payload);
          return Promise.resolve();
        case "plugin:event|emit_to":
          if (targetsThisWebview(args?.target)) deliver(args?.event, args?.payload);
          return Promise.resolve();
        case "plugin:app|version":
          return Promise.resolve(window.__PERGAMENT_VERSION__);
        case "media":
          return media(args?.url);
      }
      if (cmd.startsWith("plugin:")) {
        console.warn(`[ios] ${cmd} is not available yet`);
        return Promise.reject(unavailable);
      }
      return callNative(cmd, args);
    },
  };
})();

// Stand-in for the bridge Tauri injects on desktop (window.__TAURI_INTERNALS__).
// There is no Rust core on iOS yet: event subscriptions pretend to succeed so
// the app can mount, and every command fails with a short message that the UI
// shows as a toast.
(() => {
  if (window.__TAURI_INTERNALS__) return;

  let nextId = 1;
  const label = "main";

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

    convertFileSrc(path) {
      return `pergament://localhost/${path}`;
    },

    invoke(cmd) {
      if (cmd === "plugin:event|listen") return Promise.resolve(nextId++);
      if (cmd === "plugin:event|unlisten" || cmd === "plugin:event|emit") return Promise.resolve();
      console.warn(`[ios] ${cmd} is not available yet`);
      return Promise.reject(new Error("not available on iOS yet"));
    },
  };
})();

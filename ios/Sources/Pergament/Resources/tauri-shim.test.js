// Run with `bun test` from this folder. Not a bundle resource: Package.swift copies tauri-shim.js by name.
import { beforeEach, expect, spyOn, test } from "bun:test";

let posted;

// A fresh query per import: tests that start in the same millisecond would otherwise share one cached module, which does not run again.
let imports = 0;
beforeEach(async () => {
  globalThis.window = globalThis;
  delete window.__TAURI_INTERNALS__;
  delete window.__TAURI_EVENT_PLUGIN_INTERNALS__;
  delete window.__PERGAMENT_PLATFORM__;
  posted = [];
  window.webkit = { messageHandlers: { pergament: { postMessage: (message) => posted.push(message) } } };
  await import(`./tauri-shim.js?run=${imports++}`);
});

const internals = () => window.__TAURI_INTERNALS__;

// The call shapes @tauri-apps/api 2.12.1 uses in event.js.
async function listen(event, callback) {
  const handler = internals().transformCallback(callback);
  const eventId = await internals().invoke("plugin:event|listen", { event, target: { kind: "Any" }, handler });
  return async () => {
    window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(event, eventId);
    await internals().invoke("plugin:event|unlisten", { event, eventId });
  };
}

test("a plugin command without an iOS stand-in rejects with Tauri's plain string", async () => {
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  await expect(internals().invoke("plugin:dialog|open")).rejects.toBe("not available on iOS yet");
  expect(posted).toEqual([]);
  warn.mockRestore();
});

test("without the native handler a command rejects with the plain string", async () => {
  delete window.webkit;
  await expect(internals().invoke("render_page")).rejects.toBe("not available on iOS yet");
});

test("event commands still resolve", async () => {
  await expect(internals().invoke("plugin:event|listen", { event: "x", handler: 1 })).resolves.toBeTypeOf("number");
  await expect(internals().invoke("plugin:event|emit", { event: "x" })).resolves.toBeUndefined();
});

test("convertFileSrc keeps the path out of the web root and encodes it", () => {
  const { convertFileSrc } = internals();
  expect(convertFileSrc("/var/x#1.mp3")).toBe("asset://localhost/%2Fvar%2Fx%231.mp3");
  expect(convertFileSrc("/a b", "media")).toBe("media://localhost/%2Fa%20b");
});

test("the shim marks the platform as iOS", () => {
  expect(window.__PERGAMENT_PLATFORM__).toBe("ios");
});

test("the app version comes from the native global", async () => {
  window.__PERGAMENT_VERSION__ = "1.8.0";
  await expect(internals().invoke("plugin:app|version")).resolves.toBe("1.8.0");
  expect(posted).toEqual([]);
});

test("invoke posts the command with its arguments as a JSON string", () => {
  void internals().invoke("list_books", { query: 'a "b"' });
  void internals().invoke("library_stats");
  expect(posted).toEqual([
    { id: expect.any(Number), name: "list_books", args: '{"query":"a \\"b\\""}' },
    { id: expect.any(Number), name: "library_stats", args: "null" },
  ]);
  expect(posted[0].id).not.toBe(posted[1].id);
});

test("invoke resolves with the native reply", async () => {
  const result = internals().invoke("list_books");
  window.__pergament_reply(posted[0].id, 1, "[1]");
  await expect(result).resolves.toEqual([1]);
});

test("invoke rejects with the plain error string", async () => {
  const result = internals().invoke("list_books");
  window.__pergament_reply(posted[0].id, 0, '"boom"');
  await expect(result).rejects.toBe("boom");
});

test("replies settle their own call, and a repeated or unknown id is ignored", async () => {
  const first = internals().invoke("a");
  const second = internals().invoke("b");
  window.__pergament_reply(posted[1].id, 1, '"second"');
  window.__pergament_reply(posted[1].id, 1, '"again"');
  window.__pergament_reply(9999, 1, '"stray"');
  window.__pergament_reply(posted[0].id, 1, '"first"');
  await expect(first).resolves.toBe("first");
  await expect(second).resolves.toBe("second");
});

test("a reply meant for a previous page load does not settle the new page's call", async () => {
  void internals().invoke("list_books");
  const earlier = posted[0].id;
  await Bun.sleep(2);
  delete window.__TAURI_INTERNALS__;
  await import(`./tauri-shim.js?run=${imports++}`);
  let settled = false;
  void internals().invoke("list_books").then(() => (settled = true));
  expect(posted[1].id).not.toBe(earlier);
  window.__pergament_reply(earlier, 1, '"stale"');
  await Promise.resolve();
  expect(settled).toBe(false);
});

test("a reply that is not JSON still settles its call", async () => {
  const result = internals().invoke("list_books");
  window.__pergament_reply(posted[0].id, 1, "{");
  await expect(result).rejects.toBeTypeOf("string");
});

test("strings with quotes, backslashes and U+2028 arrive intact", async () => {
  const title = 'a "b" \\ c\u2028d ü';
  const result = internals().invoke("book_title");
  window.__pergament_reply(posted[0].id, 1, JSON.stringify(title));
  await expect(result).resolves.toBe(title);
});

test("listen delivers native events to the handler", async () => {
  const seen = [];
  await listen("progress", (e) => seen.push(e));
  await listen("other", () => seen.push("wrong"));
  window.__pergament_event("progress", '{"done":1}');
  expect(seen).toHaveLength(1);
  expect(seen[0].event).toBe("progress");
  expect(seen[0].payload).toEqual({ done: 1 });
});

test("unlisten stops delivery", async () => {
  const seen = [];
  const unlisten = await listen("progress", (e) => seen.push(e.payload));
  await listen("progress", (e) => seen.push(`kept ${e.payload}`));
  await unlisten();
  window.__pergament_event("progress", "2");
  expect(seen).toEqual(["kept 2"]);
});

test("emit and emit_to to this webview reach local listeners", async () => {
  const seen = [];
  await listen("note-link", (e) => seen.push(e.payload));
  await internals().invoke("plugin:event|emit", { event: "note-link", payload: "a" });
  await internals().invoke("plugin:event|emit_to", { target: { kind: "AnyLabel", label: "main" }, event: "note-link", payload: "b" });
  await internals().invoke("plugin:event|emit_to", { target: { kind: "AnyLabel", label: "display" }, event: "note-link", payload: "c" });
  expect(seen).toEqual(["a", "b"]);
  expect(posted).toEqual([]);
});

function fakeFetch(respond) {
  const original = globalThis.fetch;
  const calls = [];
  globalThis.fetch = async (url) => {
    calls.push(url);
    return respond(url);
  };
  return { calls, restore: () => (globalThis.fetch = original) };
}

test("media resolves with the bytes the jwmedia handler serves", async () => {
  const bytes = new Uint8Array([1, 2, 3]).buffer;
  const fetch = fakeFetch(() => ({ ok: true, status: 200, arrayBuffer: async () => bytes }));
  try {
    const result = await internals().invoke("media", { url: "jwmedia://localhost/w_E/img_1.jpg" });
    expect(new Uint8Array(result)).toEqual(new Uint8Array([1, 2, 3]));
    expect(fetch.calls).toEqual(["jwmedia://localhost/w_E/img_1.jpg"]);
    expect(posted).toEqual([]);
  } finally {
    fetch.restore();
  }
});

test("media rejects a missing picture with desktop's text", async () => {
  const fetch = fakeFetch(() => ({ ok: false, status: 404, arrayBuffer: async () => new ArrayBuffer(0) }));
  try {
    await expect(internals().invoke("media", { url: "jwmedia://localhost/w_E/no.jpg" })).rejects.toBe(
      "no image at /w_E/no.jpg",
    );
  } finally {
    fetch.restore();
  }
});

test("media rejects a failed fetch with desktop's text", async () => {
  const fetch = fakeFetch(() => {
    throw new TypeError("Load failed");
  });
  try {
    await expect(internals().invoke("media", { url: "jwmedia://localhost/catalog/x.jpg" })).rejects.toBe(
      "no image at /catalog/x.jpg",
    );
  } finally {
    fetch.restore();
  }
});

test("media rejects a URL on another scheme without fetching it", async () => {
  const fetch = fakeFetch(() => ({ ok: true, status: 200, arrayBuffer: async () => new ArrayBuffer(0) }));
  try {
    await expect(internals().invoke("media", { url: "https://example.org/x.jpg" })).rejects.toBe("not a media URL");
    expect(fetch.calls).toEqual([]);
    expect(posted).toEqual([]);
  } finally {
    fetch.restore();
  }
});

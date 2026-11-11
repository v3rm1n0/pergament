// Run with `bun test` from this folder. Not a bundle resource: Package.swift copies tauri-shim.js by name.
import { beforeEach, expect, test } from "bun:test";

// A fresh query per import: tests that start in the same millisecond would otherwise share one cached module, which does not run again.
let imports = 0;
beforeEach(async () => {
  globalThis.window = globalThis;
  delete window.__TAURI_INTERNALS__;
  await import(`./tauri-shim.js?run=${imports++}`);
});

test("an unavailable command rejects with Tauri's plain string", async () => {
  await expect(window.__TAURI_INTERNALS__.invoke("render_page")).rejects.toBe("not available on iOS yet");
});

test("event commands still resolve", async () => {
  await expect(window.__TAURI_INTERNALS__.invoke("plugin:event|listen")).resolves.toBeTypeOf("number");
  await expect(window.__TAURI_INTERNALS__.invoke("plugin:event|emit")).resolves.toBeUndefined();
});

test("convertFileSrc keeps the path out of the web root and encodes it", () => {
  const { convertFileSrc } = window.__TAURI_INTERNALS__;
  expect(convertFileSrc("/var/x#1.mp3")).toBe("asset://localhost/%2Fvar%2Fx%231.mp3");
  expect(convertFileSrc("/a b", "media")).toBe("media://localhost/%2Fa%20b");
});

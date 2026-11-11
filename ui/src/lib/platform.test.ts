// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";

const load = async (platform?: string, device?: string) => {
  vi.resetModules();
  const w = window as unknown as Record<string, unknown>;
  w.__PERGAMENT_PLATFORM__ = platform;
  w.__PERGAMENT_DEVICE__ = device;
  return import("./platform");
};

afterEach(() => {
  const w = window as unknown as Record<string, unknown>;
  delete w.__PERGAMENT_PLATFORM__;
  delete w.__PERGAMENT_DEVICE__;
});

describe("platform", () => {
  it("isIOS is false without the global", async () => {
    const { isIOS, isIPad } = await load();
    expect(isIOS).toBe(false);
    expect(isIPad).toBe(false);
  });

  it("isIOS is true when the shim sets the platform", async () => {
    expect((await load("ios", "iphone")).isIOS).toBe(true);
  });

  it("canOpenNoteWindow is false on iPhone, even when a window API exists", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = { metadata: { currentWebview: { label: "main" } } };
    const { canOpenNoteWindow, isIPad } = await load("ios", "iphone");
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    expect(isIPad).toBe(false);
    expect(canOpenNoteWindow()).toBe(false);
  });

  it("canOpenNoteWindow is true on desktop and iPad", async () => {
    expect((await load()).canOpenNoteWindow()).toBe(true);
    const ipad = await load("ios", "ipad");
    expect(ipad.isIPad).toBe(true);
    expect(ipad.canOpenNoteWindow()).toBe(true);
  });
});

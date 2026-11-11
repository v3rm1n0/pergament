import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { matches, useMediaQuery } from "./media-query";

type Listener = (e: { matches: boolean }) => void;

function stubMatchMedia(initial: boolean) {
  const listeners = new Set<Listener>();
  const mql = {
    matches: initial,
    addEventListener: (_: "change", l: Listener) => listeners.add(l),
    removeEventListener: (_: "change", l: Listener) => listeners.delete(l),
  };
  vi.stubGlobal("matchMedia", vi.fn(() => mql));
  return { mql, listeners };
}

afterEach(() => vi.unstubAllGlobals());

describe("useMediaQuery", () => {
  it("reads the initial match", () => {
    stubMatchMedia(true);
    const { result } = renderHook(() => useMediaQuery("(max-width: 719px)"));
    expect(result.current).toBe(true);
  });

  it("follows change events and unsubscribes on unmount", () => {
    const { listeners } = stubMatchMedia(false);
    const { result, unmount } = renderHook(() => useMediaQuery("(max-width: 719px)"));
    expect(listeners.size).toBe(1);
    act(() => listeners.forEach((l) => l({ matches: true })));
    expect(result.current).toBe(true);
    unmount();
    expect(listeners.size).toBe(0);
  });

  it("is false without matchMedia", () => {
    vi.stubGlobal("matchMedia", undefined);
    expect(matches("(max-width: 719px)")).toBe(false);
    const { result } = renderHook(() => useMediaQuery("(max-width: 719px)"));
    expect(result.current).toBe(false);
  });
});

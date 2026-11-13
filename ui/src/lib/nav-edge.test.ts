import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useNavEdge } from "./nav-edge";

const shell = vi.hoisted(() => ({
  isIOS: true,
  answer: null as unknown,
  invoke: vi.fn(),
  handlers: new Set<(e: { payload: unknown }) => void>(),
}));

vi.mock("./platform", () => ({
  get isIOS() {
    return shell.isIOS;
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: shell.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, handler: (e: { payload: unknown }) => void) => {
    shell.handlers.add(handler);
    return () => shell.handlers.delete(handler);
  },
}));

const send = (payload: unknown) => act(() => shell.handlers.forEach((h) => h({ payload })));

beforeEach(() => {
  shell.isIOS = true;
  shell.answer = null;
  shell.handlers.clear();
  shell.invoke.mockReset().mockImplementation(async () => shell.answer);
});

describe("useNavEdge", () => {
  it("is null off iOS and asks nothing", () => {
    shell.isIOS = false;
    const { result } = renderHook(() => useNavEdge());
    expect(result.current).toBeNull();
    expect(shell.invoke).not.toHaveBeenCalled();
  });

  it("takes the shell's answer", async () => {
    shell.answer = "right";
    const { result } = renderHook(() => useNavEdge());
    expect(result.current).toBeNull();
    await waitFor(() => expect(result.current).toBe("right"));
    expect(shell.invoke).toHaveBeenCalledWith("nav_edge");
  });

  it("follows the shell's changes and stops on unmount", async () => {
    const { result, unmount } = renderHook(() => useNavEdge());
    await waitFor(() => expect(shell.handlers.size).toBe(1));
    send("left");
    expect(result.current).toBe("left");
    send(null);
    expect(result.current).toBeNull();
    unmount();
    await waitFor(() => expect(shell.handlers.size).toBe(0));
  });

  it("reads anything but a side edge as none", async () => {
    const { result } = renderHook(() => useNavEdge());
    await waitFor(() => expect(shell.handlers.size).toBe(1));
    send("left");
    send("bottom");
    expect(result.current).toBeNull();
  });

  it("stays null when the shell cannot answer", async () => {
    shell.invoke.mockRejectedValue("not available on iOS yet");
    const { result } = renderHook(() => useNavEdge());
    await waitFor(() => expect(shell.invoke).toHaveBeenCalled());
    expect(result.current).toBeNull();
  });
});

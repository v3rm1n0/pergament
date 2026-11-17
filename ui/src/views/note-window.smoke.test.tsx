// @vitest-environment jsdom
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const calls: { cmd: string; args: unknown }[] = [];
const handlers: Record<string, (e: { payload: unknown }) => void> = {};
const emitted: { to: string; event: string; payload?: unknown }[] = [];
let closeRequested: ((e: { preventDefault: () => void }) => Promise<void>) | null = null;
const destroy = vi.fn(async () => undefined);
let resized: ((e: { payload: { toLogical: (scale: number) => { width: number; height: number } } }) => Promise<void>) | null = null;
let maximized = false;
let pendingSave: Promise<string> | null = null;
let subscriptions = 0;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    calls.push({ cmd, args });
    if (cmd === "save_note") return pendingSave ?? "n1";
    if (cmd === "list_publications") return [{ dir: "nwtsty_E", symbol: "nwtsty", langCode: "E", isBible: true }];
    if (cmd === "publication") return { toc: [], books: [{ number: 43, title: "John", chapters: 21 }] };
    if (cmd === "book_abbreviations") return { 43: "Joh" };
    return undefined;
  }),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, h: (e: { payload: unknown }) => void) => {
    handlers[event] = h;
    return () => void (handlers[event] === h && delete handlers[event]);
  }),
  emitTo: vi.fn(async (to: string, event: string, payload?: unknown) => void emitted.push({ to, event, payload })),
}));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    label: "note",
    destroy,
    onCloseRequested: async (h: typeof closeRequested) => {
      subscriptions++;
      closeRequested = h;
      return () => undefined;
    },
    onResized: async (h: typeof resized) => {
      resized = h;
      return () => undefined;
    },
    scaleFactor: async () => 1,
    isMaximized: async () => maximized,
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));

import { ask } from "@tauri-apps/plugin-dialog";
import { NoteWindowView } from "@/views/note-window";

const verse = { publication: "nwtsty_E", kind: { chapter: { book: 43, chapter: 3, verse: 16 } } };
const message = (id: number, fields?: object) => ({
  payload: { id, draft: { note: { title: "Loved the world" }, target: verse, markGuid: "m1", blockType: 2, blockIdentifier: 16 }, fields },
});
const open = (id: number, fields?: object) => act(async () => handlers["note-open"](message(id, fields)));
/** Sends a note without waiting for the window to take it, as it may be asking first. */
const send = (id: number, fields?: object) => act(async () => void handlers["note-open"](message(id, fields)));
const text = () => (screen.getByPlaceholderText("Note") as HTMLTextAreaElement).value;

beforeEach(() => {
  localStorage.clear();
  maximized = false;
  pendingSave = null;
  calls.length = 0;
  emitted.length = 0;
  destroy.mockClear();
  vi.mocked(ask).mockReset();
  render(
    <StrictMode>
      <NoteWindowView />
    </StrictMode>,
  );
});
afterEach(cleanup);

describe("Note window", () => {
  it("locks the form during a pending save", async () => {
    let resolve!: (value: string) => void;
    pendingSave = new Promise((r) => { resolve = r; });
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByPlaceholderText("Note").matches(":disabled")).toBe(true);
    await act(async () => resolve("n1"));
  });

  it("does not close a different note when an earlier save completes", async () => {
    let resolve!: (value: string) => void;
    pendingSave = new Promise((r) => { resolve = r; });
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await open(2, { title: "two", content: "second", tags: "" });
    await act(async () => resolve("n1"));
    expect(destroy).not.toHaveBeenCalled();
    expect(text()).toBe("second");
  });
  it("says it is ready, then shows the note it is sent once", async () => {
    await waitFor(() => expect(emitted.some((e) => e.to === "main" && e.event === "note-ready")).toBe(true));
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    await open(1);
    expect(text()).toBe("draft");
    expect(ask).not.toHaveBeenCalled();
  });

  it("asks the app to open a Bible link in the preview and stays open", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1, { title: "Loved the world", content: "See [Joh 3:16](pergament://bible/43:3:16-43:3:16)", tags: "" });
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    fireEvent.click(screen.getByText("Joh 3:16"));
    await waitFor(() =>
      expect(emitted).toContainEqual({ to: "main", event: "note-link", payload: "pergament://bible/43:3:16-43:3:16" }),
    );
    expect(destroy).not.toHaveBeenCalled();
  });

  it("links a reference with Ctrl+P and has the app show it, staying in front", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1, { title: "Loved the world", content: "Joh 3:16, 18", tags: "" });
    expect(screen.getByText("Ctrl+P: link and open a Bible reference")).toBeTruthy();
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    await waitFor(() => expect(calls.some((c) => c.cmd === "book_abbreviations")).toBe(true));
    await act(async () => undefined);
    area.focus();
    area.setSelectionRange(0, area.value.length);
    fireEvent.keyDown(area, { key: "p", ctrlKey: true });
    expect(area.value).toBe("[Joh 3:16](pergament://bible/43:3:16-43:3:16), [18](pergament://bible/43:3:18-43:3:18)");
    await waitFor(() =>
      expect(emitted).toContainEqual({ to: "main", event: "note-show-link", payload: "pergament://bible/43:3:16-43:3:16" }),
    );
    expect(emitted.some((e) => e.event === "note-link")).toBe(false);
  });

  it("asks once while notes keep coming in, then takes the newest", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    let answer!: (ok: boolean) => void;
    vi.mocked(ask).mockReturnValue(new Promise<boolean>((r) => (answer = r)));
    await send(2, { title: "two", content: "second", tags: "" });
    await send(3, { title: "three", content: "third", tags: "" });
    const closing = { preventDefault: vi.fn() };
    let closed!: Promise<void>;
    await act(async () => void (closed = closeRequested!(closing)));
    expect(ask).toHaveBeenCalledTimes(1);
    await act(async () => answer(true));
    await closed;
    expect(closing.preventDefault).toHaveBeenCalled();
    await waitFor(() => expect(text()).toBe("third"));
    expect(ask).toHaveBeenCalledTimes(1);
  });

  it("shows a note that came in while Escape asked about unsaved text instead of closing", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    let answer!: (ok: boolean) => void;
    vi.mocked(ask).mockReturnValue(new Promise<boolean>((r) => (answer = r)));
    fireEvent.keyDown(screen.getByPlaceholderText("Note"), { key: "Escape" });
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    await send(2, { title: "two", content: "second", tags: "" });
    await act(async () => answer(true));
    await waitFor(() => expect(text()).toBe("second"));
    expect(destroy).not.toHaveBeenCalled();
    expect(ask).toHaveBeenCalledTimes(1);
  });

  it("asks again for a note that came in while the close button asked and the text was kept", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    const answers: ((ok: boolean) => void)[] = [];
    vi.mocked(ask).mockImplementation(() => new Promise<boolean>((r) => answers.push(r)));
    const closing = { preventDefault: vi.fn() };
    let closed!: Promise<void>;
    await act(async () => void (closed = closeRequested!(closing)));
    await send(2, { title: "two", content: "second", tags: "" });
    await act(async () => answers[0](false));
    await closed;
    expect(closing.preventDefault).toHaveBeenCalled();
    expect(text()).toBe("draft");
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(2));
    await act(async () => answers[1](true));
    await waitFor(() => expect(text()).toBe("second"));
    expect(destroy).not.toHaveBeenCalled();
  });

  it("subscribes to its window's events once, not on every render", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    const before = subscriptions;
    await open(1);
    await open(2);
    expect(subscriptions).toBe(before);
  });

  it("starts with text moved from the study pane and counts it as unsaved", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1, { title: "Loved the world", content: "moved", tags: "" });
    expect((screen.getByPlaceholderText("Note") as HTMLTextAreaElement).value).toBe("moved");
    await waitFor(() => expect(emitted.some((e) => e.event === "note-dirty" && e.payload === true)).toBe(true));
  });

  it("saves to the page the note was started on, tells the app and closes", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "God gave his Son" } });
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(destroy).toHaveBeenCalled());
    expect(calls.find((c) => c.cmd === "save_note")!.args).toEqual({
      target: verse,
      note: { title: "Loved the world", content: "God gave his Son", tags: [], guid: undefined, blockType: 2, blockIdentifier: 16, markGuid: "m1" },
    });
    expect(emitted.some((e) => e.to === "main" && e.event === "notes-changed")).toBe(true);
  });

  it("asks before the close button drops unsaved text, and not without changes", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    const clean = { preventDefault: vi.fn() };
    await act(() => closeRequested!(clean));
    expect(ask).not.toHaveBeenCalled();
    expect(clean.preventDefault).not.toHaveBeenCalled();

    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(false);
    const dirty = { preventDefault: vi.fn() };
    await act(() => closeRequested!(dirty));
    expect(ask).toHaveBeenCalledTimes(1);
    expect(dirty.preventDefault).toHaveBeenCalled();
  });

  it("asks before another note replaces unsaved text", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(false);
    await open(2);
    expect((screen.getByPlaceholderText("Note") as HTMLTextAreaElement).value).toBe("draft");
    vi.mocked(ask).mockResolvedValue(true);
    await open(3);
    expect((screen.getByPlaceholderText("Note") as HTMLTextAreaElement).value).toBe("");
  });

  it("asks before another note replaces moved text that came in just before it", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    vi.mocked(ask).mockResolvedValue(false);
    await act(async () => {
      void handlers["note-open"](message(1, { title: "Loved the world", content: "moved", tags: "" }));
      void handlers["note-open"](message(2));
    });
    expect(ask).toHaveBeenCalledTimes(1);
    expect(text()).toBe("moved");
    expect(emitted).toContainEqual({ to: "main", event: "note-dirty", payload: true });
  });

  it("closes on Escape, asking first when there are changes", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(true);
    fireEvent.keyDown(screen.getByPlaceholderText("Note"), { key: "Escape" });
    await waitFor(() => expect(destroy).toHaveBeenCalled());
    expect(ask).toHaveBeenCalledTimes(1);
  });

  it("follows the theme and language chosen in the app", async () => {
    await waitFor(() => expect(handlers["note-open"]).toBeTruthy());
    await open(1);
    expect(document.documentElement.classList.contains("dark")).toBe(false);
    localStorage.setItem("theme", "dark");
    localStorage.setItem("uiLang", "de");
    act(() => void window.dispatchEvent(new StorageEvent("storage", { key: "theme" })));
    act(() => void window.dispatchEvent(new StorageEvent("storage", { key: "uiLang" })));
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(screen.getByPlaceholderText("Titel")).toBeTruthy();
    localStorage.clear();
    act(() => void window.dispatchEvent(new StorageEvent("storage", { key: "uiLang" })));
    expect(screen.getByPlaceholderText("Title")).toBeTruthy();
  });

  it("remembers its size, but not the maximized one", async () => {
    await waitFor(() => expect(resized).toBeTruthy());
    const size = (width: number, height: number) => ({ payload: { toLogical: () => ({ width, height }) } });
    await act(() => resized!(size(600, 500)));
    expect(JSON.parse(localStorage.getItem("noteWindowSize")!)).toEqual({ width: 600, height: 500 });
    maximized = true;
    await act(() => resized!(size(1920, 1080)));
    expect(JSON.parse(localStorage.getItem("noteWindowSize")!)).toEqual({ width: 600, height: 500 });
  });
});

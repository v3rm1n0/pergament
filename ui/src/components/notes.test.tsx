// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined), emitTo: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: "main", onCloseRequested: async () => () => undefined }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { NoteCard, NoteForm } from "./notes";
import type { Note } from "@/lib/api";

const note: Note = {
  guid: "a",
  title: "Long one",
  content: "Line **one**\n\nSee [map](https://example.org/)",
  blockType: 0,
  blockIdentifier: null,
  markGuid: null,
  color: 1,
  tags: [],
  lastModified: "2026-10-03T00:00:00Z",
  location: null,
};

const layout = ["scrollHeight", "clientHeight"].map((key) => [key, Object.getOwnPropertyDescriptor(HTMLElement.prototype, key)] as const);

afterEach(() => {
  cleanup();
  for (const [key, original] of layout) {
    if (original) Object.defineProperty(HTMLElement.prototype, key, original);
    else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key];
  }
});

/** jsdom does no layout; make the clamped text report that it is cut off. */
function clip(cut: boolean) {
  Object.defineProperty(HTMLElement.prototype, "scrollHeight", { configurable: true, get: () => (cut ? 200 : 50) });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => 50 });
}

describe("NoteCard", () => {
  it("offers to show the whole note only when it is cut off", () => {
    clip(false);
    render(<NoteCard note={note} onClick={() => undefined} onToggle={() => undefined} />);
    expect(screen.queryByRole("button", { name: "Show whole note" })).toBeNull();
    cleanup();
    clip(true);
    render(<NoteCard note={note} onClick={() => undefined} onToggle={() => undefined} />);
    expect(screen.getByRole("button", { name: "Show whole note" })).toBeTruthy();
  });

  it("measures again when the card changes size", () => {
    const resized: (() => void)[] = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          resized.push(callback);
        }
        observe() {}
        disconnect() {}
      },
    );
    try {
      clip(false);
      render(<NoteCard note={note} onClick={() => undefined} onToggle={() => undefined} />);
      expect(screen.queryByRole("button", { name: "Show whole note" })).toBeNull();
      clip(true);
      act(() => resized.forEach((f) => f()));
      expect(screen.getByRole("button", { name: "Show whole note" })).toBeTruthy();
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("toggles without editing, and opens links of the whole note in the browser", () => {
    clip(true);
    const onClick = vi.fn();
    const onToggle = vi.fn();
    const { rerender } = render(<NoteCard note={note} onClick={onClick} onToggle={onToggle} />);
    fireEvent.click(screen.getByRole("button", { name: "Show whole note" }));
    expect(onToggle).toHaveBeenCalledTimes(1);
    expect(onClick).not.toHaveBeenCalled();
    rerender(<NoteCard note={note} expanded onClick={onClick} onToggle={onToggle} />);
    expect(screen.getByRole("button", { name: "Show less" })).toBeTruthy();
    fireEvent.click(screen.getByText("map"));
    expect(onClick).not.toHaveBeenCalled();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("open_external", { url: "https://example.org/" });
    fireEvent.click(screen.getByText("one"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("does not edit the note when text in the whole note is selected", () => {
    const onClick = vi.fn();
    render(<NoteCard note={note} expanded onClick={onClick} onToggle={() => undefined} />);
    const selection = vi.spyOn(window, "getSelection").mockReturnValue({ isCollapsed: false } as Selection);
    fireEvent.click(screen.getByText("one"));
    expect(onClick).not.toHaveBeenCalled();
    selection.mockRestore();
  });

  const linked: Note = { ...note, content: "Read [Mt 24:14](pergament://bible/40:24:14-40:24:14) and [map](https://example.org/)" };

  it("opens the app's own links in the app and others in the browser, without editing", () => {
    vi.mocked(invoke).mockClear();
    const onClick = vi.fn();
    const onAppLink = vi.fn();
    render(<NoteCard note={linked} expanded onClick={onClick} onAppLink={onAppLink} />);
    fireEvent.click(screen.getByText("Mt 24:14"));
    expect(onAppLink).toHaveBeenCalledWith("pergament://bible/40:24:14-40:24:14");
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("open_external", expect.anything());
    fireEvent.click(screen.getByText("map"));
    expect(onAppLink).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("open_external", { url: "https://example.org/" });
    expect(onClick).not.toHaveBeenCalled();
  });

  it("asks the main window to open the app's own links when no one else does", () => {
    vi.mocked(emitTo).mockClear();
    render(<NoteCard note={linked} expanded />);
    fireEvent.click(screen.getByText("Mt 24:14"));
    expect(vi.mocked(emitTo)).toHaveBeenCalledWith("main", "note-link", "pergament://bible/40:24:14-40:24:14");
  });

  it("names the title button of an untitled whole note so it can be edited from the keyboard", () => {
    const onClick = vi.fn();
    render(<NoteCard note={{ ...note, title: "" }} expanded onClick={onClick} onToggle={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: "Edit note" }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});

describe("NoteForm", () => {
  const books = [{ number: 40, title: "Matthäus", chapters: 28 }];
  /** Writes the text, selects `start`..`end` and presses Ctrl+L. */
  function link(text: string, start: number, end: number, props: { books?: typeof books | null } = { books }) {
    render(<NoteForm note={{ content: text }} focus={false} onSave={() => undefined} onCancel={() => undefined} {...props} />);
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    area.focus();
    area.setSelectionRange(start, end);
    fireEvent.keyDown(area, { key: "l", ctrlKey: true });
    return area;
  }

  it("links a selected Bible reference with Ctrl+L", () => {
    const area = link("See Mt 24:14 now", 4, 12);
    expect(area.value).toBe("See [Mt 24:14](pergament://bible/40:24:14-40:24:14) now");
    expect(area.selectionStart).toBe(area.value.indexOf(" now"));
  });

  it("offers Insert link on a long press, which links like Ctrl+L", () => {
    vi.useFakeTimers();
    try {
      render(<NoteForm note={{ content: "See Mt 24:14 now" }} focus={false} onSave={() => undefined} onCancel={() => undefined} books={books} />);
      const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
      area.setSelectionRange(4, 12);
      fireEvent.pointerDown(area, { pointerType: "touch", clientX: 5, clientY: 5 });
      act(() => void vi.advanceTimersByTime(500));
      fireEvent.click(screen.getByRole("menuitem", { name: "Insert link" }));
      expect(area.value).toBe("See [Mt 24:14](pergament://bible/40:24:14-40:24:14) now");
    } finally {
      vi.useRealTimers();
    }
  });

  it("links a range of verses, leaving spaces around the selection outside", () => {
    expect(link(" Mt 24:14-16 ", 0, 13).value).toBe(" [Mt 24:14-16](pergament://bible/40:24:14-40:24:16) ");
  });

  it("leaves the address empty for other text, with the caret in it", () => {
    const area = link("a word", 2, 6);
    expect(area.value).toBe("a [word]()");
    expect([area.selectionStart, area.selectionEnd]).toEqual([9, 9]);
  });

  it("links no book name without a chapter", () => {
    expect(link("Ruth", 0, 4, { books: [...books, { number: 8, title: "Ruth", chapters: 4 }] }).value).toBe("[Ruth]()");
  });

  it("keeps a link on one line", () => {
    expect(link("Mt\n24:14", 0, 8).value).toBe("[Mt 24:14](pergament://bible/40:24:14-40:24:14)");
  });

  /** Writes the text, selects `start`..`end` and presses Ctrl+P in a form that can open links. */
  function linkAndOpen(text: string, start: number, end: number, key: { key: string; ctrlKey?: boolean; metaKey?: boolean } = { key: "p", ctrlKey: true }) {
    const onOpenLink = vi.fn();
    render(<NoteForm note={{ content: text }} focus={false} onSave={() => undefined} onCancel={() => undefined} books={books} onOpenLink={onOpenLink} />);
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    area.focus();
    area.setSelectionRange(start, end);
    const passed = fireEvent.keyDown(area, key);
    return { area, onOpenLink, passed };
  }

  it("links a reference with Ctrl+P and opens it", () => {
    const { area, onOpenLink, passed } = linkAndOpen("See Mt 24:14 now", 4, 12);
    expect(passed).toBe(false);
    expect(area.value).toBe("See [Mt 24:14](pergament://bible/40:24:14-40:24:14) now");
    expect(onOpenLink).toHaveBeenCalledExactlyOnceWith("pergament://bible/40:24:14-40:24:14");
  });

  it("opens the first of several verses with Ctrl+P, and takes Cmd+P on a Mac", () => {
    const { area, onOpenLink } = linkAndOpen("Mt 24:14, 16; 25:1", 0, 18, { key: "p", metaKey: true });
    expect(area.value).toBe(
      "[Mt 24:14](pergament://bible/40:24:14-40:24:14), [16](pergament://bible/40:24:16-40:24:16); [25:1](pergament://bible/40:25:1-40:25:1)",
    );
    expect(onOpenLink).toHaveBeenCalledExactlyOnceWith("pergament://bible/40:24:14-40:24:14");
  });

  it("links other text with Ctrl+P but opens nothing", () => {
    const { area, onOpenLink } = linkAndOpen("a word", 2, 6);
    expect(area.value).toBe("a [word]()");
    expect(onOpenLink).not.toHaveBeenCalled();
  });

  it("leaves Ctrl+P alone where links cannot be opened", () => {
    render(<NoteForm note={{ content: "Mt 24:14" }} focus={false} onSave={() => undefined} onCancel={() => undefined} books={books} />);
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    area.setSelectionRange(0, 8);
    expect(fireEvent.keyDown(area, { key: "p", ctrlKey: true })).toBe(true);
    expect(area.value).toBe("Mt 24:14");
    expect(screen.queryByText("Ctrl+P: link and open a Bible reference")).toBeNull();
  });

  it("takes Cmd+L on a Mac, and leaves the title alone", () => {
    render(<NoteForm note={{ title: "Mt 24:14", content: "Mt 24:14" }} focus={false} onSave={() => undefined} onCancel={() => undefined} books={books} />);
    const title = screen.getByPlaceholderText<HTMLInputElement>("Title");
    title.setSelectionRange(0, 8);
    expect(fireEvent.keyDown(title, { key: "l", ctrlKey: true })).toBe(true);
    expect(title.value).toBe("Mt 24:14");
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    area.setSelectionRange(0, 8);
    expect(fireEvent.keyDown(area, { key: "l", metaKey: true })).toBe(false);
    expect(area.value).toBe("[Mt 24:14](pergament://bible/40:24:14-40:24:14)");
  });

  it("types the link through the browser where it can, so it can be undone", () => {
    const exec = vi.fn((_: string, __: boolean, text: string) => {
      const area = document.activeElement as HTMLTextAreaElement;
      area.setRangeText(text, area.selectionStart, area.selectionEnd, "end");
      return true;
    });
    Object.defineProperty(document, "execCommand", { configurable: true, value: exec });
    try {
      const area = link("a word", 2, 6);
      expect(exec).toHaveBeenCalledWith("insertText", false, "[word]()");
      expect(area.value).toBe("a [word]()");
      expect([area.selectionStart, area.selectionEnd]).toEqual([9, 9]);
    } finally {
      delete (document as unknown as Record<string, unknown>).execCommand;
    }
  });

  it("links each part of a list of verses, keeping the text between them", () => {
    const joh = [...books, { number: 43, title: "Johannes", chapters: 21 }];
    const href = (a: number, b = a) => `pergament://bible/43:15:${a}-43:15:${b}`;
    expect(link("Joh 15:13,15", 0, 12, { books: joh }).value).toBe(`[Joh 15:13](${href(13)}),[15](${href(15)})`);
    cleanup();
    const area = link("See Joh 15:13, 15-17.", 4, 20, { books: joh });
    expect(area.value).toBe(`See [Joh 15:13](${href(13)}), [15-17](${href(15, 17)}).`);
    expect(area.selectionStart).toBe(area.value.length - 1);
    cleanup();
    expect(link("Joh 15:13-14,16", 0, 15, { books: joh }).value).toBe(`[Joh 15:13-14](${href(13, 14)}),[16](${href(16)})`);
  });

  it("links each chapter of a list after semicolons", () => {
    const joh = [...books, { number: 43, title: "Johannes", chapters: 21 }];
    const href = (c: number, a: number, b = a) => `pergament://bible/43:${c}:${a}-43:${c}:${b}`;
    expect(link("Joh 15:13, 15; 16:2-4", 0, 21, { books: joh }).value).toBe(
      `[Joh 15:13](${href(15, 13)}), [15](${href(15, 15)}); [16:2-4](${href(16, 2, 4)})`,
    );
    cleanup();
    expect(link("Joh 15; 16:2", 0, 12, { books: joh }).value).toBe("[Joh 15; 16:2]()");
  });

  it("links nothing of a list when one of its verses does not fit", () => {
    const joh = [...books, { number: 43, title: "Johannes", chapters: 21 }];
    expect(link("Joh 15:13,0", 0, 11, { books: joh }).value).toBe("[Joh 15:13,0]()");
  });

  it("reads no reference without a Bible", () => {
    expect(link("Mt 24:14", 0, 8, { books: null }).value).toBe("[Mt 24:14]()");
  });

  it("inserts an empty link with the caret in its text when nothing is selected", () => {
    const area = link("ab", 1, 1);
    expect(area.value).toBe("a[]()b");
    expect([area.selectionStart, area.selectionEnd]).toEqual([2, 2]);
  });

  it("opens the app's own links in the preview in the app and others in the browser", () => {
    vi.mocked(invoke).mockClear();
    const onAppLink = vi.fn();
    const content = "[Mt 24:14](pergament://bible/40:24:14-40:24:14) [map](https://example.org/)";
    render(<NoteForm note={{ content }} focus={false} onSave={() => undefined} onCancel={() => undefined} onAppLink={onAppLink} />);
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    fireEvent.click(screen.getByText("Mt 24:14"));
    expect(onAppLink).toHaveBeenCalledWith("pergament://bible/40:24:14-40:24:14");
    fireEvent.click(screen.getByText("map"));
    expect(onAppLink).toHaveBeenCalledTimes(1);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("open_external", { url: "https://example.org/" });
  });

  it("leaves Ctrl+Shift+L alone", () => {
    render(<NoteForm note={{ content: "x" }} focus={false} onSave={() => undefined} onCancel={() => undefined} books={books} />);
    const area = screen.getByPlaceholderText<HTMLTextAreaElement>("Note");
    area.setSelectionRange(0, 1);
    fireEvent.keyDown(area, { key: "L", ctrlKey: true, shiftKey: true });
    expect(area.value).toBe("x");
  });
});

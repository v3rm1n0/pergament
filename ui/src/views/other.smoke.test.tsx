// @vitest-environment jsdom
// Renders the search, meetings, updates and publication views in StrictMode with a mocked backend.
import { StrictMode, useEffect, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setUiLang } from "@/lib/i18n";
import { locales } from "@/locales";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { card, entry } from "@/lib/fixtures";

const calls: { cmd: string; args: unknown }[] = [];
let backend: Record<string, (args: any) => unknown> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    calls.push({ cmd, args });
    const handler = backend[cmd];
    if (!handler) throw new Error(`unmocked command ${cmd}`);
    return handler(args);
  }),
  convertFileSrc: (p: string) => p,
}));
/** The latest handler per event, so a test can play the note window's messages. */
const listeners: Record<string, (e: { payload: unknown }) => void> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, h: (e: { payload: unknown }) => void) => {
    listeners[event] = h;
    return () => undefined;
  }),
  emitTo: vi.fn(async () => undefined),
}));
let closeRequested: ((e: { preventDefault: () => void }) => Promise<void>) | null = null;
const setFocus = vi.fn(async () => undefined);
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    label: "main",
    unminimize: async () => undefined,
    setFocus,
    onCloseRequested: async (h: typeof closeRequested) => {
      closeRequested = h;
      return () => undefined;
    },
  }),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider, useApp } from "@/app";
import { MeetingsView } from "@/views/meetings";
import { OnlineView } from "@/views/online";
import { PublicationView } from "@/views/publication";
import { UpdatesTab } from "@/views/updates";
import { SettingsView } from "@/views/settings";
import { PersonalView } from "@/views/personal";
import { GoTo } from "@/components/go-to";
import { InlineNoteEditor } from "@/components/notes";
import { AppUpdate } from "@/components/app-update";
import { ask, message, open } from "@tauri-apps/plugin-dialog";
import { emitTo } from "@tauri-apps/api/event";

beforeEach(() => {
  calls.length = 0;
  closeRequested = null;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  backend = {
    list_publications: () => [card({ dir: "lff_2", symbol: "lff", title: "Glücklich" })],
    languages: () => [],
    catalog_cached: () => true,
    check_updates: () => null,
    dated_page: () => null,
    catalog_search: () => [entry("ll", "Brochures and Booklets", null, "Höre auf Gott"), entry("lff", "Books", "lff_2", "Glücklich")],
    meetings: () => ({ workbook: null, study: null, other: [entry("sjj", "Songbooks", null, "Singt voller Freude")] }),
    publication: () => ({ card: card({ dir: "lff_2", symbol: "lff", title: "Glücklich" }), toc: [{ title: "Root", document_id: null, bible_book: null, children: [] }], books: [] }),
  };
  Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => undefined) } });
});
afterEach(cleanup);

const renderIn = (ui: React.ReactNode) =>
  render(
    <StrictMode>
      <AppProvider>{ui}</AppProvider>
    </StrictMode>,
  );
const tile = async (title: string) => (await screen.findByText(title)).closest(".group") as HTMLElement;
// Radix opens a menu on a pointer press, not on a click.
const openMenu = (t: HTMLElement) => fireEvent.pointerDown(within(t).getByLabelText("More"), { button: 0, ctrlKey: false });

describe("Search", () => {
  it("gives result cards a menu, and Remove only for downloaded ones", async () => {
    renderIn(<OnlineView />);
    fireEvent.submit(screen.getByRole("textbox").closest("form")!);
    openMenu(await tile("Höre auf Gott"));
    expect(screen.queryByText("Remove")).toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    openMenu(await tile("Glücklich"));
    expect(screen.getByText("Remove")).toBeTruthy();
  });
});

describe("Meetings", () => {
  it("gives the other meeting publications a menu", async () => {
    renderIn(<MeetingsView />);
    openMenu(await tile("Singt voller Freude"));
    expect(screen.getByText("Share Link")).toBeTruthy();
  });

  it("shows the publications right away when the view is opened again", async () => {
    const first = renderIn(<MeetingsView />);
    await screen.findByText("Singt voller Freude");
    first.unmount();
    renderIn(<MeetingsView />);
    expect(screen.getByText("Singt voller Freude")).toBeTruthy();
  });
});

describe("Updates", () => {
  it("lists an update with a menu that can remove the installed version", async () => {
    backend.check_updates = () => [
      { dir: "lff_2", langCode: "X", installedSize: 1, entry: entry("lff", "Books", "lff_2", "Glücklich") },
    ];
    renderIn(<UpdatesTab />);
    expect(within((await screen.findByText("Check now")).closest("div")!).getByText("Needs network")).toBeTruthy();
    fireEvent.click(await screen.findByText("Check now"));
    openMenu(await tile("Glücklich"));
    expect(screen.getByText("Remove")).toBeTruthy();
  });
});

describe("Publication", () => {
  it("toggles the favorite star and keeps it in sync with the stored list", async () => {
    renderIn(<PublicationView dir="lff_2" />);
    fireEvent.click(await screen.findByLabelText("Add to favorites"));
    await waitFor(() => expect(JSON.parse(localStorage.getItem("favorites")!)).toEqual(["lff_2"]));
    fireEvent.click(await screen.findByLabelText("Remove from favorites"));
    await waitFor(() => expect(JSON.parse(localStorage.getItem("favorites")!)).toEqual([]));
  });

  it("shows a favorite that was added elsewhere as set", async () => {
    localStorage.setItem("favorites", JSON.stringify(["lff_2"]));
    renderIn(<PublicationView dir="lff_2" />);
    expect(await screen.findByLabelText("Remove from favorites")).toBeTruthy();
  });
});

describe("Settings", () => {
  it("switches the content width and remembers it", async () => {
    renderIn(<SettingsView />);
    expect(document.documentElement.style.getPropertyValue("--page-max")).toBe("880px");
    // The selected tab outlives the view, so pick it explicitly.
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Interface" }));
    fireEvent.click(await screen.findByRole("radio", { name: "Full width" }));
    expect(localStorage.getItem("layout")).toBe("wide");
    expect(document.documentElement.style.getPropertyValue("--page-max")).toBe("100000px");
    expect(screen.getByRole("radio", { name: "Full width" }).getAttribute("aria-checked")).toBe("true");
  });

  it("picks the interface language from a list", async () => {
    renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Interface" }));
    const trigger = await screen.findByRole("button", { name: "Interface language" });
    // Radix opens the menu on a pointer press, not on a click.
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: "mouse" });
    const items = await screen.findAllByRole("menuitemradio");
    expect(items.map((i) => i.textContent)).toEqual(["System", "English", ...locales.map((l) => l.name)]);
    expect(items[0].getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByRole("menuitemradio", { name: "Deutsch" }));
    expect(localStorage.getItem("uiLang")).toBe("de");
    setUiLang("en"); // the language is module state; the other tests read English
  });

  it("switches to the easy-to-read font and remembers it", async () => {
    renderIn(<SettingsView />);
    expect(document.documentElement.dataset.font).toBe("standard");
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Reading" }));
    fireEvent.click(await screen.findByRole("radio", { name: "Easy to read" }));
    expect(localStorage.getItem("readingFont")).toBe("legible");
    expect(document.documentElement.dataset.font).toBe("legible");
  });

  it("groups the settings into four tabs", async () => {
    renderIn(<SettingsView />);
    for (const name of ["Interface", "Reading", "Library & data", "About"]) {
      expect(await screen.findByRole("tab", { name })).toBeTruthy();
    }
    expect(screen.getByRole("tablist", { name: "Settings" })).toBeTruthy();
    fireEvent.mouseDown(screen.getByRole("tab", { name: "Library & data" }));
    expect(await screen.findByRole("heading", { name: "Backup" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Library & data" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tab", { name: "Interface" }).getAttribute("aria-selected")).toBe("false");
    fireEvent.mouseDown(screen.getByRole("tab", { name: "About" }));
    expect(await screen.findByText(/unofficial reader/)).toBeTruthy();
    expect(screen.getByRole("tab", { name: "About" }).getAttribute("aria-selected")).toBe("true");
  });

  it("remembers the selected tab when the view comes back", async () => {
    const first = renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Library & data" }));
    expect(await screen.findByRole("heading", { name: "Backup" })).toBeTruthy();
    first.unmount();
    renderIn(<SettingsView />);
    const tab = await screen.findByRole("tab", { name: "Library & data" });
    expect(tab.getAttribute("aria-selected")).toBe("true");
    expect(await screen.findByRole("heading", { name: "Backup" })).toBeTruthy();
  });
});

describe("Settings cache", () => {
  const usage = { catalog: 58_000_000, covers: 2_000_000, partial: 0, other: 400_000, total: 60_400_000 };
  const open = async () => {
    backend.cache_usage = () => usage;
    backend.clear_cache = () => null;
    renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Library & data" }));
    expect(await screen.findByText("60 MB")).toBeTruthy();
  };
  const count = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

  it("keeps the cache when the confirmation is declined", async () => {
    vi.mocked(ask).mockResolvedValue(false);
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Clear cache" }));
    await waitFor(() => expect(ask).toHaveBeenCalled());
    expect(count("clear_cache")).toBe(0);
  });

  it("clears the cache once confirmed and measures it again", async () => {
    vi.mocked(ask).mockResolvedValue(true);
    await open();
    const before = count("cache_usage");
    fireEvent.click(screen.getByRole("button", { name: "Clear cache" }));
    await waitFor(() => expect(count("clear_cache")).toBe(1));
    await waitFor(() => expect(count("cache_usage")).toBeGreaterThan(before));
  });

  it("says so and keeps Clear disabled when the cache cannot be measured", async () => {
    backend.cache_usage = () => {
      throw new Error("permission denied");
    };
    renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Library & data" }));
    expect(await screen.findByText("Cannot measure the cache")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Clear cache" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("waits for running downloads before clearing", async () => {
    backend.cache_usage = () => usage;
    backend.download_publication = () => new Promise(() => undefined);
    const Start = () => {
      const { download } = useApp();
      return <button onClick={() => void download(entry("lff", "Books", null, "Glücklich"))}>start</button>;
    };
    renderIn(
      <>
        <Start />
        <SettingsView />
      </>,
    );
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Library & data" }));
    expect(await screen.findByText("60 MB")).toBeTruthy();
    const clear = screen.getByRole("button", { name: "Clear cache" }) as HTMLButtonElement;
    expect(clear.disabled).toBe(false);
    fireEvent.click(screen.getByText("start"));
    expect(await screen.findByText("Wait for the running downloads to finish.")).toBeTruthy();
    expect(clear.disabled).toBe(true);
  });
});

describe("Personal Study", () => {
  const note = (guid: string, title: string, color: number | null, lastModified: string) => ({
    guid, title, content: "", blockType: 0, blockIdentifier: null, markGuid: null, color, tags: [], lastModified, location: null,
  });

  it("narrows the notes by text and highlight color and changes their order", async () => {
    backend.user_data_summary = () => ({ marks: 0, notes: 2, tags: 0, bookmarks: 0, device: null });
    backend.tags = () => [];
    backend.bookmarks = () => [];
    backend.all_notes = () => [note("a", "Dusk visits", 1, "2026-10-03T00:00:00Z"), note("b", "Patient listening", 3, "2026-10-05T00:00:00Z")];
    renderIn(<PersonalView />);
    const titles = () => screen.getAllByText(/Dusk visits|Patient listening/).map((e) => e.textContent);
    await screen.findByText("Dusk visits");
    expect(titles()).toEqual(["Patient listening", "Dusk visits"]);
    fireEvent.keyDown(screen.getByRole("combobox", { name: "Sort notes" }), { key: "Enter" });
    fireEvent.click(await screen.findByRole("option", { name: "Oldest first" }));
    expect(titles()).toEqual(["Dusk visits", "Patient listening"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "patient" } });
    expect(titles()).toEqual(["Patient listening"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Yellow" }));
    expect(titles()).toEqual(["Dusk visits"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "zzz" } });
    expect(screen.getByText("No notes match.")).toBeTruthy();
  });

  it("shows a note's Markdown formatted, with links only underlined", async () => {
    backend.user_data_summary = () => ({ marks: 0, notes: 1, tags: 0, bookmarks: 0, device: null });
    backend.tags = () => [];
    backend.bookmarks = () => [];
    backend.all_notes = () => [
      { ...note("a", "Dusk visits", 1, "2026-10-03T00:00:00Z"), content: "Bring **tea**\n- cups\n- [map](https://example.org)\n<b>raw</b>" },
    ];
    renderIn(<PersonalView />);
    const tea = await screen.findByText("tea");
    const card = tea.closest(".note-md")!;
    expect(tea.tagName).toBe("STRONG");
    expect(card.querySelectorAll("li")).toHaveLength(2);
    expect(card.querySelector("a")).toBeNull();
    expect(within(card as HTMLElement).getByText("map").className).toBe("md-link");
    expect(card.querySelector("b")).toBeNull();
    expect(card.textContent).toContain("<b>raw</b>");
  });
});

describe("Note in the study pane", () => {
  const verse = { publication: "nwtsty_E", kind: { chapter: { book: 43, chapter: 3, verse: 16 } } };

  /** Opens a new note on John 3:16, then lets the test move elsewhere in the app. */
  function Harness() {
    const { view, openNote, releaseNote, userVersion, notesVersion } = useApp();
    return (
      <>
        <span data-testid="view">{view.name}</span>
        <span data-testid="versions">{`${userVersion}/${notesVersion}`}</span>
        <button onClick={() => void openNote({ note: { title: "Loved the world" }, target: verse, markGuid: "m1", blockType: 2, blockIdentifier: 16 }, { in: "pane" })}>
          open
        </button>
        <button onClick={() => void openNote({ note: { title: "Another" } }, { in: "pane" })}>other</button>
        <button onClick={releaseNote}>leave</button>
      </>
    );
  }

  beforeEach(() => vi.mocked(ask).mockReset());

  it("opens a note in the note window unless asked for the study pane", async () => {
    backend.open_note_window = () => undefined;
    function Opener() {
      const { openNote } = useApp();
      return <button onClick={() => void openNote({ note: { title: "Elsewhere" } })}>window</button>;
    }
    renderIn(
      <>
        <Opener />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("window"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    await waitFor(() =>
      expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ draft: { note: { title: "Elsewhere" } } })),
    );
    expect(screen.queryByRole("form")).toBeNull();
  });

  it("asks before closing the main window with unsaved text", async () => {
    renderIn(
      <>
        <Harness />
        <InlineNoteEditor />
      </>,
    );
    await waitFor(() => expect(closeRequested).toBeTruthy());
    const clean = { preventDefault: vi.fn() };
    await act(() => closeRequested!(clean));
    expect(ask).not.toHaveBeenCalled();
    expect(clean.preventDefault).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(false);
    const dirty = { preventDefault: vi.fn() };
    await act(() => closeRequested!(dirty));
    expect(ask).toHaveBeenCalledTimes(1);
    expect(dirty.preventDefault).toHaveBeenCalled();
  });

  it("asks before closing the main window when only the note window holds unsaved text", async () => {
    renderIn(<Harness />);
    await waitFor(() => expect(closeRequested && listeners["note-dirty"]).toBeTruthy());
    act(() => listeners["note-dirty"]({ payload: true }));
    vi.mocked(ask).mockResolvedValue(false);
    const dirty = { preventDefault: vi.fn() };
    await act(() => closeRequested!(dirty));
    expect(ask).toHaveBeenCalledTimes(1);
    expect(dirty.preventDefault).toHaveBeenCalled();
  });

  it("saves to the page it was started on", async () => {
    backend.save_note = () => "n1";
    renderIn(
      <>
        <Harness />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "God gave his Son" } });
    fireEvent.click(within(screen.getByRole("form", { name: "New note" })).getByText("Save"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "save_note")).toBe(true));
    expect(calls.find((c) => c.cmd === "save_note")!.args).toEqual({
      target: verse,
      note: { title: "Loved the world", content: "God gave his Son", tags: [], guid: undefined, blockType: 2, blockIdentifier: 16, markGuid: "m1" },
    });
    expect(screen.queryByRole("form")).toBeNull();
    // Only the notes signal moves; the study panes key on userVersion and would remount.
    expect(screen.getByTestId("versions").textContent).toBe("0/1");
  });

  it("previews Markdown and opens its links in the browser", async () => {
    backend.open_external = () => undefined;
    renderIn(
      <>
        <Harness />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), {
      target: { value: "**Love** for [all](https://example.org/)\n- first\n- second" },
    });
    fireEvent.click(within(editor).getByRole("button", { name: "Preview" }));
    const preview = within(editor).getByTestId("note-preview");
    expect(preview.querySelector("strong")?.textContent).toBe("Love");
    expect(preview.querySelectorAll("li")).toHaveLength(2);
    fireEvent.click(within(preview).getByText("all"));
    await waitFor(() => expect(calls.find((c) => c.cmd === "open_external")?.args).toEqual({ url: "https://example.org/" }));
    fireEvent.click(within(editor).getByRole("button", { name: "Write" }));
    expect((within(editor).getByPlaceholderText("Note") as HTMLTextAreaElement).value).toContain("**Love**");
  });

  const openEditor = async () => {
    renderIn(
      <>
        <Harness />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    return screen.findByRole("form", { name: "New note" });
  };

  it("focuses the title once it is shown", async () => {
    const editor = await openEditor();
    await waitFor(() => expect(document.activeElement).toBe(within(editor).getByPlaceholderText("Title")));
  });

  it("closes right away without changes and asks before Close, Cancel or Escape drop text", async () => {
    let editor = await openEditor();
    fireEvent.click(within(editor).getByText("Cancel"));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(ask).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("open"));
    editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.keyDown(within(editor).getByPlaceholderText("Note"), { key: "Escape" });
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    await act(async () => undefined);
    fireEvent.click(within(editor).getByText("Cancel"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(2));
    await act(async () => undefined);
    expect(screen.getByRole("form")).toBeTruthy();
    vi.mocked(ask).mockResolvedValue(true);
    fireEvent.click(within(editor).getByText("Cancel"));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
  });

  it("asks only once when two requests come in while the question is open", async () => {
    const editor = await openEditor();
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "draft" } });
    let answer!: (ok: boolean) => void;
    vi.mocked(ask).mockReturnValue(new Promise<boolean>((r) => (answer = r)));
    fireEvent.click(screen.getByText("other"));
    fireEvent.click(screen.getByText("other"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    await act(async () => answer(false));
    expect(ask).toHaveBeenCalledTimes(1);
  });

  it("does not stack a second question when the window is closed while one is open", async () => {
    const editor = await openEditor();
    await waitFor(() => expect(closeRequested).toBeTruthy());
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "draft" } });
    let answer!: (ok: boolean) => void;
    vi.mocked(ask).mockReturnValue(new Promise<boolean>((r) => (answer = r)));
    fireEvent.click(screen.getByText("other"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    const closing = { preventDefault: vi.fn() };
    let closed!: Promise<void>;
    await act(async () => void (closed = closeRequested!(closing)));
    expect(ask).toHaveBeenCalledTimes(1);
    await act(async () => answer(false));
    await closed;
    expect(closing.preventDefault).toHaveBeenCalled();
  });

  it("keeps the note open when saving fails, and saves once on a double click", async () => {
    let fail = true;
    backend.save_note = () => {
      if (fail) throw new Error("disk full");
      return "n1";
    };
    const editor = await openEditor();
    fireEvent.click(within(editor).getByText("Save"));
    expect(await screen.findByText(/disk full/)).toBeTruthy();
    expect(screen.getByRole("form")).toBeTruthy();
    fail = false;
    calls.length = 0;
    fireEvent.click(within(editor).getByText("Save"));
    fireEvent.click(within(editor).getByText("Save"));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(calls.filter((c) => c.cmd === "save_note")).toHaveLength(1);
  });

  it("moves the note with its unsaved text to the window by button and by Ctrl+Shift+O in the text", async () => {
    backend.open_note_window = () => undefined;
    let editor = await openEditor();
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "half written" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    await waitFor(() =>
      expect(vi.mocked(emitTo)).toHaveBeenCalledWith(
        "note",
        "note-open",
        expect.objectContaining({ fields: { title: "Loved the world", content: "half written", tags: "" }, draft: expect.objectContaining({ markGuid: "m1" }) }),
      ),
    );
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(ask).not.toHaveBeenCalled();
    // The window saved that note and closed.
    act(() => listeners["note-closed"]({ payload: undefined }));

    vi.mocked(emitTo).mockClear();
    fireEvent.click(screen.getByText("open"));
    editor = await screen.findByRole("form", { name: "New note" });
    const text = within(editor).getByPlaceholderText("Note");
    fireEvent.change(text, { target: { value: "again" } });
    // fireEvent returns false when the handler cancelled the default action (typing the letter).
    expect(fireEvent.keyDown(text, { key: "O", ctrlKey: true, shiftKey: true })).toBe(false);
    await waitFor(() => expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.objectContaining({ content: "again" }) })));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
  });

  it("moves unsaved text to the window when its place goes away and just closes an unchanged note", async () => {
    backend.open_note_window = () => undefined;
    let editor = await openEditor();
    fireEvent.click(screen.getByText("leave"));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(calls.some((c) => c.cmd === "open_note_window")).toBe(false);

    fireEvent.click(screen.getByText("open"));
    editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "keep me" } });
    fireEvent.click(screen.getByText("leave"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.objectContaining({ content: "keep me" }) }));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(ask).not.toHaveBeenCalled();
  });

  it("keeps the text in the pane when the note window cannot open", async () => {
    backend.open_note_window = () => {
      throw new Error("no window");
    };
    const editor = await openEditor();
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "precious" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    expect(await screen.findByText(/no window/)).toBeTruthy();
    expect(screen.getByDisplayValue("precious")).toBeTruthy();
    expect(screen.getByRole("form", { name: "New note" })).toBeTruthy();
    expect(ask).not.toHaveBeenCalled();
  });

  it("keeps the text in the pane while the note window holds unsaved text of its own", async () => {
    backend.open_note_window = () => undefined;
    const editor = await openEditor();
    await waitFor(() => expect(listeners["note-dirty"]).toBeTruthy());
    act(() => listeners["note-dirty"]({ payload: true }));
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "mine" } });
    vi.mocked(emitTo).mockClear();
    fireEvent.click(within(editor).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    expect(await screen.findByText("Save or close the note in the note window first.")).toBeTruthy();
    // The window the message speaks of comes forward.
    expect(calls.filter((c) => c.cmd === "open_note_window")).toHaveLength(1);
    fireEvent.click(screen.getByText("leave"));
    await act(async () => undefined);
    expect(vi.mocked(emitTo)).not.toHaveBeenCalledWith("note", "note-open", expect.anything());
    expect(screen.getByDisplayValue("mine")).toBeTruthy();
    expect(screen.getByRole("form", { name: "New note" })).toBeTruthy();
  });

  it("does not send another note over moved text the note window has not taken yet", async () => {
    backend.open_note_window = () => undefined;
    function Opener() {
      const { openNote } = useApp();
      return <button onClick={() => void openNote({ note: { title: "Elsewhere" } })}>window</button>;
    }
    renderIn(
      <>
        <Harness />
        <Opener />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "moved" } });
    fireEvent.click(screen.getByText("leave"));
    await waitFor(() => expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.anything() })));
    vi.mocked(emitTo).mockClear();
    calls.length = 0;
    fireEvent.click(screen.getByText("window"));
    expect(await screen.findByText("Save or close the note in the note window first.")).toBeTruthy();
    // The window the message speaks of comes forward, and still gets the moved text when it gets ready.
    expect(calls.filter((c) => c.cmd === "open_note_window")).toHaveLength(1);
    await act(async () => listeners["note-ready"]({ payload: undefined }));
    expect(vi.mocked(emitTo)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.objectContaining({ content: "moved" }) }));
  });

  it("sends another note to the note window holding its own unsaved text, which asks about it", async () => {
    backend.open_note_window = () => undefined;
    function Opener() {
      const { openNote } = useApp();
      return <button onClick={() => void openNote({ note: { title: "Elsewhere" } })}>window</button>;
    }
    renderIn(<Opener />);
    await waitFor(() => expect(listeners["note-dirty"]).toBeTruthy());
    act(() => listeners["note-dirty"]({ payload: true }));
    vi.mocked(emitTo).mockClear();
    fireEvent.click(screen.getByText("window"));
    await waitFor(() =>
      expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ draft: { note: { title: "Elsewhere" } } })),
    );
    expect(screen.queryByText("Save or close the note in the note window first.")).toBeNull();
  });

  it("keeps a second note in the pane while the first one's unsaved text has not reached the new window", async () => {
    backend.open_note_window = () => undefined;
    let editor = await openEditor();
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "first" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    // The window has not said yet that it holds the first text.
    fireEvent.click(screen.getByText("other"));
    editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "second" } });
    vi.mocked(emitTo).mockClear();
    fireEvent.click(within(editor).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    expect(await screen.findByText("Save or close the note in the note window first.")).toBeTruthy();
    expect(vi.mocked(emitTo)).not.toHaveBeenCalledWith("note", "note-open", expect.anything());
    expect(screen.getByDisplayValue("second")).toBeTruthy();
    // Once the window reports, its own state counts.
    act(() => listeners["note-dirty"]({ payload: false }));
    fireEvent.click(within(screen.getByRole("form", { name: "New note" })).getByRole("button", { name: "Open in window (Ctrl+Shift+O)" }));
    await waitFor(() => expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.objectContaining({ content: "second" }) })));
  });

  /** Shows the editor in another place on each page, as the reader's study pane does when the page changes. */
  function Pages() {
    const { releaseNote } = useApp();
    const [page, setPage] = useState(1);
    useEffect(() => releaseNote, [page, releaseNote]);
    return (
      <>
        <button onClick={() => setPage((p) => p + 1)}>next page</button>
        {page % 2 ? (
          <div>
            <InlineNoteEditor />
          </div>
        ) : (
          <section>
            <InlineNoteEditor />
          </section>
        )}
      </>
    );
  }

  it("hides the editor while its text moves to the window and brings the text back when the window cannot open", async () => {
    let fail!: (e: Error) => void;
    backend.open_note_window = () => new Promise((_, reject) => (fail = reject));
    renderIn(
      <>
        <Harness />
        <Pages />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "precious" } });
    fireEvent.click(screen.getByText("next page"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    // Nothing typed while the window opens can get lost.
    expect(screen.queryByRole("form")).toBeNull();
    await act(async () => fail(new Error("no window")));
    expect(await screen.findByText(/no window/)).toBeTruthy();
    expect(screen.getByDisplayValue("precious")).toBeTruthy();
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.click(within(screen.getByRole("form", { name: "New note" })).getByText("Cancel"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
  });

  it("asks before another note replaces the unsaved text that is moving to the window", async () => {
    let opened!: () => void;
    backend.open_note_window = () => new Promise<void>((r) => (opened = r));
    renderIn(
      <>
        <Harness />
        <Pages />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "moving" } });
    fireEvent.click(screen.getByText("next page"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.click(screen.getByText("other"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    await act(async () => opened());
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
    expect(vi.mocked(emitTo)).toHaveBeenCalledWith("note", "note-open", expect.objectContaining({ fields: expect.objectContaining({ content: "moving" }) }));
  });

  it("keeps track of unsaved text in a note opened while another one moves to the window", async () => {
    let opened!: () => void;
    backend.open_note_window = () => new Promise<void>((r) => (opened = r));
    renderIn(
      <>
        <Harness />
        <Pages />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "moving" } });
    fireEvent.click(screen.getByText("next page"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    vi.mocked(ask).mockResolvedValue(true);
    fireEvent.click(screen.getByText("other"));
    await screen.findByDisplayValue("Another");
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "typed meanwhile" } });
    await act(async () => opened());
    vi.mocked(ask).mockClear();
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.click(within(screen.getByRole("form")).getByText("Cancel"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(screen.getByDisplayValue("typed meanwhile")).toBeTruthy();
  });

  it("brings the text back on the new page when the note window holds unsaved text", async () => {
    renderIn(
      <>
        <Harness />
        <Pages />
      </>,
    );
    await waitFor(() => expect(listeners["note-dirty"]).toBeTruthy());
    act(() => listeners["note-dirty"]({ payload: true }));
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "precious" } });
    fireEvent.click(screen.getByText("next page"));
    expect(await screen.findByText("Save or close the note in the note window first.")).toBeTruthy();
    expect(await screen.findByDisplayValue("precious")).toBeTruthy();
  });

  it("keeps a preview link click from reaching the pane around the editor", async () => {
    backend.open_external = () => undefined;
    const outer = vi.fn();
    renderIn(
      <div onClick={outer}>
        <Harness />
        <InlineNoteEditor />
      </div>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    outer.mockClear();
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "[all](https://example.org/)" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Preview" }));
    outer.mockClear();
    fireEvent.click(within(within(editor).getByTestId("note-preview")).getByText("all"));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "open_external")).toHaveLength(1));
    expect(outer).not.toHaveBeenCalled();
  });

  it("does not mark a newer note as saved when an older save finishes late", async () => {
    let finish!: () => void;
    backend.save_note = () => new Promise<string>((r) => (finish = () => r("n1")));
    const editor = await openEditor();
    fireEvent.click(within(editor).getByText("Save"));
    // While the save runs, open another note and start typing in it.
    fireEvent.click(screen.getByText("other"));
    const other = await screen.findByDisplayValue("Another");
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "unsaved" } });
    await act(async () => finish());
    expect(screen.getByDisplayValue("Another")).toBe(other);
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.click(screen.getByText("Cancel"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(screen.getByDisplayValue("unsaved")).toBeTruthy();
  });

  it("asks before another note replaces unsaved text", async () => {
    renderIn(
      <>
        <Harness />
        <InlineNoteEditor />
      </>,
    );
    fireEvent.click(screen.getByText("other"));
    fireEvent.click(screen.getByText("open"));
    await screen.findByDisplayValue("Loved the world");
    expect(ask).not.toHaveBeenCalled();
    fireEvent.change(screen.getByPlaceholderText("Note"), { target: { value: "draft" } });
    vi.mocked(ask).mockResolvedValue(false);
    fireEvent.click(screen.getByText("other"));
    await waitFor(() => expect(ask).toHaveBeenCalledTimes(1));
    expect(screen.getByDisplayValue("Loved the world")).toBeTruthy();
    vi.mocked(ask).mockResolvedValue(true);
    fireEvent.click(screen.getByText("other"));
    expect(await screen.findByDisplayValue("Another")).toBeTruthy();
  });
});

describe("Merging backups", () => {
  beforeEach(() => {
    vi.mocked(message).mockReset();
    vi.mocked(ask).mockReset();
    vi.mocked(open).mockReset();
  });
  const restoreSummary = { marks: 0, notes: 0, tags: 0, bookmarks: 0, device: null };
  const emptyUser = () => {
    backend.user_data_summary = () => ({ marks: 0, notes: 0, tags: 0, bookmarks: 0, device: null });
    backend.tags = () => [];
    backend.bookmarks = () => [];
    backend.all_notes = () => [];
  };
  const dataTab = async () => {
    renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "Library & data" }));
  };
  const report = (conflicts: { table: string; guid: string; summary: string }[]) => ({
    added: 2, updated: 1, unchanged: 0, conflicts, device: null,
  });

  it("is off by default and the checkbox is remembered", async () => {
    await dataTab();
    const box = await screen.findByRole("switch", { name: "Merge backups instead of replacing" });
    expect(box.getAttribute("aria-checked")).toBe("false");
    expect(screen.getByRole("button", { name: "Restore backup…" })).toBeTruthy();
    fireEvent.click(box);
    expect(box.getAttribute("aria-checked")).toBe("true");
    expect(localStorage.getItem("mergeBackups")).toBe("true");
    expect(screen.getByRole("button", { name: "Merge backup…" })).toBeTruthy();
  });

  it("switches the button label in Personal Study too", async () => {
    emptyUser();
    renderIn(<PersonalView />);
    expect(await screen.findByRole("button", { name: "Restore backup…" })).toBeTruthy();
    cleanup();
    localStorage.setItem("mergeBackups", "true");
    renderIn(<PersonalView />);
    expect(await screen.findByRole("button", { name: "Merge backup…" })).toBeTruthy();
  });

  it("merges instead of restoring when it is on, and does nothing when declined", async () => {
    localStorage.setItem("mergeBackups", "true");
    backend.merge_backup = () => report([]);
    backend.restore_backup = () => restoreSummary;
    vi.mocked(open).mockResolvedValue("/tmp/b.jwlibrary");
    vi.mocked(ask).mockResolvedValue(false);
    await dataTab();
    fireEvent.click(await screen.findByRole("button", { name: "Merge backup…" }));
    await waitFor(() => expect(ask).toHaveBeenCalled());
    expect(vi.mocked(ask).mock.calls[0][1]).toEqual({ title: "Merge backup?", kind: "info", okLabel: "Merge" });
    // Let the handler finish with the declined answer.
    await act(async () => {
      await vi.mocked(ask).mock.results[0].value;
    });
    expect(calls.some((c) => c.cmd === "merge_backup" || c.cmd === "restore_backup")).toBe(false);

    vi.mocked(ask).mockResolvedValue(true);
    fireEvent.click(screen.getByRole("button", { name: "Merge backup…" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "merge_backup")).toBe(true));
    expect(calls.find((c) => c.cmd === "merge_backup")!.args).toEqual({ path: "/tmp/b.jwlibrary" });
    expect(calls.some((c) => c.cmd === "restore_backup")).toBe(false);
    expect(await screen.findByText("Merged 3 changes")).toBeTruthy();
    expect(message).not.toHaveBeenCalled();
  });

  it("shows the conflicts in a dialog", async () => {
    localStorage.setItem("mergeBackups", "true");
    backend.merge_backup = () => report([{ table: "Note", guid: "g1", summary: "Dusk visits" }]);
    vi.mocked(open).mockResolvedValue("/tmp/b.jwlibrary");
    vi.mocked(ask).mockResolvedValue(true);
    await dataTab();
    fireEvent.click(await screen.findByRole("button", { name: "Merge backup…" }));
    await waitFor(() => expect(message).toHaveBeenCalled());
    expect(vi.mocked(message).mock.calls[0]).toEqual([
      "1 conflict, your version was kept:\nNote: Dusk visits",
      { title: "Conflicts", kind: "warning" },
    ]);
  });

  it("does not call a merge failed when only the conflict dialog fails", async () => {
    localStorage.setItem("mergeBackups", "true");
    backend.merge_backup = () => report([{ table: "Note", guid: "g1", summary: "Dusk visits" }]);
    vi.mocked(open).mockResolvedValue("/tmp/b.jwlibrary");
    vi.mocked(ask).mockResolvedValue(true);
    vi.mocked(message).mockRejectedValue(new Error("no window"));
    await dataTab();
    fireEvent.click(await screen.findByRole("button", { name: "Merge backup…" }));
    await waitFor(() => expect(message).toHaveBeenCalled());
    expect(await screen.findByText("Merged 3 changes")).toBeTruthy();
    await act(async () => {
      await vi.mocked(message).mock.results[0].value.catch(() => undefined);
    });
    expect(screen.queryByText(/Merge failed/)).toBeNull();
  });

  it("restores instead of merging when it is off", async () => {
    backend.merge_backup = () => report([]);
    backend.restore_backup = () => restoreSummary;
    vi.mocked(open).mockResolvedValue("/tmp/b.jwlibrary");
    vi.mocked(ask).mockResolvedValue(true);
    await dataTab();
    fireEvent.click(await screen.findByRole("button", { name: "Restore backup…" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "restore_backup")).toBe(true));
    expect(calls.find((c) => c.cmd === "restore_backup")!.args).toEqual({ path: "/tmp/b.jwlibrary" });
    expect(calls.some((c) => c.cmd === "merge_backup")).toBe(false);
    expect(vi.mocked(ask).mock.calls[0][1]).toEqual({ title: "Restore backup?", kind: "warning", okLabel: "Restore" });
  });

  it("ignores a second restore while one is running", async () => {
    localStorage.setItem("mergeBackups", "true");
    let finish: (r: unknown) => void = () => undefined;
    backend.merge_backup = () => new Promise((resolve) => (finish = resolve));
    vi.mocked(open).mockResolvedValue("/tmp/b.jwlibrary");
    vi.mocked(ask).mockResolvedValue(true);
    await dataTab();
    const button = await screen.findByRole("button", { name: "Merge backup…" });
    fireEvent.click(button);
    await waitFor(() => expect(calls.filter((c) => c.cmd === "merge_backup")).toHaveLength(1));
    fireEvent.click(button);
    fireEvent.click(button);
    await act(async () => {
      await Promise.resolve();
    });
    expect(open).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish(report([]));
    });
    expect(await screen.findByText("Merged 3 changes")).toBeTruthy();
    // Once it is done, the next one runs.
    fireEvent.click(button);
    await waitFor(() => expect(calls.filter((c) => c.cmd === "merge_backup")).toHaveLength(2));
    expect(open).toHaveBeenCalledTimes(2);
  });
});

describe("Go to", () => {
  it("opens with Ctrl+G and goes to the typed reference", async () => {
    backend.list_publications = () => [card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Bible", isBible: true, langCode: "X" })];
    backend.publication = () => ({
      card: card({ dir: "nwtsty_2", symbol: "nwtsty", isBible: true }),
      toc: [],
      books: [{ number: 58, title: "Hebräer", chapter_title: "", book_document_id: null, chapters: 13 }],
    });
    backend.last_verse = () => 39;
    renderIn(<GoTo />);
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_publications")).toBe(true));
    await new Promise((r) => setTimeout(r, 50));
    fireEvent.keyDown(window, { key: "g", ctrlKey: true });
    const box = await screen.findByLabelText("Reference, for example Heb 10:24");
    await waitFor(() => expect((screen.getByRole("button", { name: "Go" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.change(box, { target: { value: "Zzz 1" } });
    fireEvent.submit(box.closest("form")!);
    expect(await screen.findByText("No such place in this Bible.")).toBeTruthy();
    // Asking again keeps the first place to return to, Tab stays in the box and Escape closes it.
    fireEvent.keyDown(window, { key: "g", ctrlKey: true });
    const go = screen.getByRole("button", { name: "Go" });
    go.focus();
    fireEvent.keyDown(go, { key: "Tab" });
    expect(document.activeElement).toBe(box);
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.keyDown(window, { key: "g", ctrlKey: true });
    const again = await screen.findByLabelText("Reference, for example Heb 10:24");
    await waitFor(() => expect((screen.getByRole("button", { name: "Go" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.change(again, { target: { value: "Heb 10:999" } });
    fireEvent.submit(again.closest("form")!);
    expect(await screen.findByText("No such place in this Bible.")).toBeTruthy();
    fireEvent.change(again, { target: { value: "Heb 10:24" } });
    fireEvent.submit(again.closest("form")!);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

describe("Links from the note window", () => {
  it("opens them in the Bible chosen for the language, in front, and says when one is not in the library", async () => {
    backend.list_publications = () => [card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Bible", isBible: true, langCode: "X" })];
    backend.link_action = ({ href }: { href: string }) =>
      href.includes("bible")
        ? { kind: "open", target: { publication: "nwtsty_2", kind: { chapter: { book: 40, chapter: 24, verse: 14 } } }, studyNote: false }
        : { kind: "missing", url: null };
    function View() {
      return <span data-testid="view">{useApp().view.name}</span>;
    }
    renderIn(<View />);
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_publications")).toBe(true));
    await waitFor(() => expect(listeners["note-link"]).toBeTruthy());
    setFocus.mockClear();
    await act(async () => listeners["note-link"]({ payload: "pergament://bible/40:24:14-40:24:14" }));
    await waitFor(() => expect(screen.getByTestId("view").textContent).toBe("reader"));
    expect(calls.filter((c) => c.cmd === "link_action").map((c) => c.args)).toEqual([
      { current: "nwtsty_2", href: "pergament://bible/40:24:14-40:24:14" },
    ]);
    expect(setFocus).toHaveBeenCalled();
    await act(async () => listeners["note-link"]({ payload: "pergament://pub/X:1/1" }));
    expect(await screen.findByText("Not in your library: pergament://pub/X:1/1")).toBeTruthy();
    expect(calls.filter((c) => c.cmd === "link_action")).toHaveLength(2);
    // Only the app's own links come this way.
    await act(async () => listeners["note-link"]({ payload: "https://example.org/" }));
    await act(async () => listeners["note-link"]({ payload: 42 }));
    expect(calls.filter((c) => c.cmd === "link_action")).toHaveLength(2);
  });

  it("shows those opened while writing without taking the note window's place in front", async () => {
    backend.list_publications = () => [card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Bible", isBible: true, langCode: "X" })];
    backend.link_action = () => ({
      kind: "open",
      target: { publication: "nwtsty_2", kind: { chapter: { book: 40, chapter: 24, verse: 14 } } },
      studyNote: false,
    });
    function View() {
      return <span data-testid="view">{useApp().view.name}</span>;
    }
    renderIn(<View />);
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_publications")).toBe(true));
    await waitFor(() => expect(listeners["note-show-link"]).toBeTruthy());
    setFocus.mockClear();
    await act(async () => listeners["note-show-link"]({ payload: "pergament://bible/40:24:14-40:24:14" }));
    await waitFor(() => expect(screen.getByTestId("view").textContent).toBe("reader"));
    expect(setFocus).not.toHaveBeenCalled();
    await act(async () => listeners["note-show-link"]({ payload: "https://example.org/" }));
    expect(calls.filter((c) => c.cmd === "link_action")).toHaveLength(1);
  });
});

describe("Coming back to a page", () => {
  it("shows the publication right away", async () => {
    const first = renderIn(<PublicationView dir="lff_2" />);
    await screen.findByLabelText("Add to favorites");
    first.unmount();
    renderIn(<PublicationView dir="lff_2" />);
    expect(screen.getByLabelText("Add to favorites")).toBeTruthy();
  });

  it("shows the notes of Personal Study right away", async () => {
    backend.user_data_summary = () => ({ marks: 0, notes: 1, tags: 0, bookmarks: 0, device: null });
    backend.tags = () => [];
    backend.bookmarks = () => [];
    backend.all_notes = () => [
      { guid: "a", title: "Dusk visits", content: "", blockType: 0, blockIdentifier: null, markGuid: null, color: null, tags: [], lastModified: "2026-10-03T00:00:00Z", location: null },
    ];
    const first = renderIn(<PersonalView />);
    await screen.findByText("Dusk visits");
    first.unmount();
    renderIn(<PersonalView />);
    expect(screen.getByText("Dusk visits")).toBeTruthy();
  });

  it("brings back the last search", async () => {
    const first = renderIn(<OnlineView />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Höre" } });
    fireEvent.submit(screen.getByRole("textbox").closest("form")!);
    await screen.findByText("Höre auf Gott");
    first.unmount();
    renderIn(<OnlineView />);
    expect(screen.getByText("Höre auf Gott")).toBeTruthy();
    expect((screen.getByRole("textbox") as HTMLInputElement).value).toBe("Höre");
  });
});

describe("Settings app update", () => {
  const update = { version: "1.7.0", notes: "## What's Changed\n* faster search", url: "https://github.com/v3rm1n0/pergament/releases/tag/v1.7.0", installable: true };
  const ran = (cmd: string) => calls.some((c) => c.cmd === cmd);

  beforeEach(() => {
    backend.check_app_update = () => ({ kind: "zip", available: update });
    backend.install_app_update = () => undefined;
    backend.running_jobs = () => 0;
    backend.restart_app = () => undefined;
    backend.open_external = () => undefined;
  });
  afterEach(() => vi.useRealTimers());

  it("sits in the About tab", async () => {
    renderIn(<SettingsView />);
    fireEvent.mouseDown(await screen.findByRole("tab", { name: "About" }));
    expect(await screen.findByRole("button", { name: "Check for updates" })).toBeTruthy();
  });

  it("tells a Nix install to update through Nix", async () => {
    backend.check_app_update = () => ({ kind: "nix", available: null });
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Pergament was installed with Nix. Update it through your Nix configuration.")).toBeTruthy();
    expect(ran("install_app_update")).toBe(false);
  });

  it("says when Pergament is up to date", async () => {
    backend.check_app_update = () => ({ kind: "zip", available: null });
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Pergament is up to date")).toBeTruthy();
  });

  it("says when the check fails", async () => {
    backend.check_app_update = () => {
      throw "HTTP 403";
    };
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Could not check for updates: HTTP 403")).toBeTruthy();
  });

  it("shows the version and notes and installs on confirm", async () => {
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    const dialog = await screen.findByRole("dialog", { name: "Pergament 1.7.0 is available" });
    expect(within(dialog).getByRole("heading", { name: "What's Changed" })).toBeTruthy();
    expect(within(dialog).getByText("faster search")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Update and restart" }));
    await waitFor(() => expect(ran("restart_app")).toBe(true));
    expect(calls.find((c) => c.cmd === "install_app_update")?.args).toEqual({ version: "1.7.0" });
  });

  it("closes without installing on Later", async () => {
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Later" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(ran("install_app_update")).toBe(false);
  });

  it("offers the download page when it cannot install", async () => {
    backend.check_app_update = () => ({ kind: "manual", available: { ...update, installable: false } });
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Open download page" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "open_external")?.args).toEqual({ url: update.url }));
    expect(ran("install_app_update")).toBe(false);
    expect(screen.queryByRole("button", { name: "Update and restart" })).toBeNull();
  });

  it("keeps the app running when the install fails", async () => {
    backend.install_app_update = () => {
      throw "signature does not match";
    };
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Update and restart" }));
    expect(await screen.findByText("Could not install the update: signature does not match")).toBeTruthy();
    expect(ran("restart_app")).toBe(false);
  });

  it("waits for running downloads before restarting", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let jobs = 1;
    backend.running_jobs = () => jobs;
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Update and restart" }));
    expect(await screen.findByText("Waiting for 1 downloads or imports to finish…")).toBeTruthy();
    await act(() => vi.advanceTimersByTimeAsync(2000));
    expect(ran("restart_app")).toBe(false);
    jobs = 0;
    await act(() => vi.advanceTimersByTimeAsync(1100));
    await waitFor(() => expect(ran("restart_app")).toBe(true));
  });

  it("restarts right away when asked not to wait", async () => {
    backend.running_jobs = () => 1;
    renderIn(<AppUpdate />);
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Update and restart" }));
    fireEvent.click(await screen.findByRole("button", { name: "Restart now" }));
    await waitFor(() => expect(ran("restart_app")).toBe(true));
  });

  it("does not restart over an unsaved note", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    function Opener() {
      const { openNote } = useApp();
      return <button onClick={() => void openNote({ note: { title: "Draft" } }, { in: "pane" })}>open</button>;
    }
    backend.running_jobs = () => 1;
    renderIn(
      <>
        <Opener />
        <InlineNoteEditor />
        <AppUpdate />
      </>,
    );
    fireEvent.click(screen.getByText("open"));
    const editor = await screen.findByRole("form", { name: "New note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    fireEvent.click(await screen.findByRole("button", { name: "Update and restart" }));
    expect(await screen.findByText("Save or close your note to restart.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Restart now" })).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(ran("restart_app")).toBe(false);
  });
});

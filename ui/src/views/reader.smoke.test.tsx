// @vitest-environment jsdom
// Renders the reader in StrictMode with a mocked backend: the study pane's note editor and the page changes that move it.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { card } from "@/lib/fixtures";
import type { Bookmark, Note, Target } from "@/lib/api";

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
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    label: "main",
    onCloseRequested: async () => () => undefined,
    unminimize: async () => undefined,
    setFocus: async () => undefined,
  }),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider, useApp } from "@/app";
import { ReaderView } from "@/views/reader";
import { chapterTarget, documentTarget } from "@/lib/api";
import { emitTo } from "@tauri-apps/api/event";

const note: Note = {
  guid: "n1",
  title: "Loved the world",
  content: "God gave his Son",
  blockType: 2,
  blockIdentifier: 16,
  markGuid: null,
  color: 1,
  tags: [],
  lastModified: "2026-10-03T00:00:00Z",
  location: null,
};

beforeEach(() => {
  // jsdom does no layout or scrolling.
  Element.prototype.scrollTo = () => undefined;
  Element.prototype.scrollIntoView = () => undefined;
  calls.length = 0;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  backend = {
    list_publications: () => [card({ dir: "nwtsty_E", symbol: "nwtsty", title: "Study Bible" })],
    languages: () => [],
    catalog_cached: () => true,
    check_updates: () => null,
    dated_page: () => null,
    publication: () => ({
      card: card({ dir: "nwtsty_E", symbol: "nwtsty", title: "Study Bible" }),
      toc: [],
      books: [{ number: 43, name: "John", chapters: 21 }],
    }),
    render_page: ({ target }: { target: Target }) => {
      const c = "chapter" in target.kind ? target.kind.chapter.chapter : 0;
      return { title: `John ${c}`, html: `<p><span class="v" id="v43-${c}-16-1">For God loved the world</span></p>`, fragment: null, name: "" };
    },
    chapter_study: () => ({ outlineTitle: null, outline: [], verses: [] }),
    research_verses: () => [],
    page_user_data: ({ target }: { target: Target }) => ({
      marks: [],
      bookmarks: [],
      notes: "chapter" in target.kind && target.kind.chapter.chapter === 3 ? [note] : [],
      answers: {},
    }),
    publication_bookmarks: () => [],
    save_bookmark: () => undefined,
    open_note_window: () => undefined,
  };
});
afterEach(cleanup);

/** Shows where the app went, which the reader's page keys change. */
function Where() {
  const { view } = useApp();
  return <span data-testid="where">{JSON.stringify(view)}</span>;
}

const reader = (target: Target, fromSearch = false) => (
  <StrictMode>
    <AppProvider>
      <ReaderView target={target} fromSearch={fromSearch} />
      <Where />
    </AppProvider>
  </StrictMode>
);

describe("Reader", () => {
  it("bookmarks the whole chapter by default, even when scrolled", async () => {
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await screen.findByLabelText("Show note");
    const text = screen.getByText("For God loved the world");
    const scroller = text.closest(".overflow-y-auto")!;
    scroller.scrollTop = 500;
    fireEvent.scroll(scroller);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 1" }));
    await waitFor(() => expect(calls).toContainEqual({
      cmd: "save_bookmark",
      args: {
        target: chapterTarget("nwtsty_E", 43, 3),
        bookmark: { slot: 0, blockType: 0, blockIdentifier: null, snippet: null },
      },
    }));
  });

  it("bookmarks a clicked verse without requiring a text selection", async () => {
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await screen.findByLabelText("Show note");
    fireEvent.click(screen.getByText("For God loved the world"));
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 2" }));
    await waitFor(() => expect(calls).toContainEqual({
      cmd: "save_bookmark",
      args: {
        target: chapterTarget("nwtsty_E", 43, 3),
        bookmark: { slot: 1, blockType: 2, blockIdentifier: 16, snippet: "For God loved the world" },
      },
    }));
  });

  it("bookmarks a clicked paragraph, then resets to the whole section on its heading", async () => {
    backend.render_page = () => ({
      title: "Article", html: '<h1>Section heading</h1><p id="p7">A paragraph</p>',
      fragment: null, name: "",
    });
    backend.page_user_data = () => ({ marks: [], notes: [], bookmarks: [], answers: {} });
    render(reader(documentTarget("nwtsty_E", 1)));
    await screen.findByText("A paragraph");
    fireEvent.click(screen.getByText("A paragraph"));
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 1" }));
    await waitFor(() => expect(calls).toContainEqual({
      cmd: "save_bookmark",
      args: {
        target: documentTarget("nwtsty_E", 1),
        bookmark: { slot: 0, blockType: 1, blockIdentifier: 7, snippet: "A paragraph" },
      },
    }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByText("Section heading"));
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 2" }));
    await waitFor(() => expect(calls).toContainEqual({
      cmd: "save_bookmark",
      args: {
        target: documentTarget("nwtsty_E", 1),
        bookmark: { slot: 1, blockType: 0, blockIdentifier: null, snippet: null },
      },
    }));
  });

  it("shows page and verse bookmark colors and refreshes them after saving", async () => {
    let saved: Bookmark[] = [{
      slot: 0, title: "John 3", snippet: null, blockType: 0, blockIdentifier: null,
      location: { keySymbol: "nwtsty", mepsLanguage: 0, issueTag: 0, documentId: null, book: 43, chapter: 3, title: "John 3" },
    }];
    backend.page_user_data = () => ({ marks: [], notes: [], bookmarks: [...saved], answers: {} });
    backend.publication_bookmarks = () => [...saved];
    backend.delete_bookmark = ({ slot }: { slot: number }) => {
      saved = saved.filter((b) => b.slot !== slot);
    };
    backend.save_bookmark = ({ bookmark }: { bookmark: Pick<Bookmark, "slot" | "blockType" | "blockIdentifier" | "snippet"> }) => {
      saved.push({ ...saved[0], ...bookmark });
    };
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await waitFor(() => expect(document.querySelector(".reader .bookmark-page [data-slot='0']")).toBeTruthy());
    const root = document.querySelector(".reader")!;
    expect(root.querySelector<HTMLElement>(".bookmark-marker")?.style.backgroundColor).toBe("rgb(181, 45, 105)");
    fireEvent.click(screen.getByText("For God loved the world"));
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 2" }));
    await waitFor(() => expect(root.querySelector(".bookmark-marker[data-slot='1']")).toBeTruthy());
    expect(root.querySelectorAll(".bookmark-marker")).toHaveLength(2);
    expect(root.querySelector<HTMLElement>(".bookmark-marker[data-slot='1']")?.style.backgroundColor).toBe("rgb(132, 87, 166)");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.pointerDown(await screen.findByRole("button", { name: "Bookmark 2 options" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(root.querySelector(".bookmark-marker[data-slot='1']")).toBeNull());
    expect(root.querySelectorAll(".bookmark-marker")).toHaveLength(1);
  });

  it("shows multiple bookmarks beside the same paragraph without changing its text", async () => {
    backend.render_page = () => ({
      title: "Article", html: '<h1>Section heading</h1><p id="p7">A paragraph</p>',
      fragment: null, name: "",
    });
    backend.page_user_data = () => ({
      marks: [], notes: [], answers: {},
      bookmarks: [1, 4].map((slot) => ({ slot, title: "Paragraph", blockType: 1, blockIdentifier: 7 })),
    });
    render(reader(documentTarget("nwtsty_E", 1)));
    await waitFor(() => expect(document.querySelectorAll(".reader .bookmark-marker")).toHaveLength(2));
    expect(document.querySelector("#p7")?.textContent).toBe("A paragraph");
    expect(document.querySelectorAll(".bookmark-markers")).toHaveLength(1);
  });

  it("positions publication bookmarks at the lesson title and paragraph despite nested layouts", async () => {
    backend.render_page = () => ({
      title: "Lesson", html: '<figure>Cover image</figure><header><h1>Lesson title</h1></header><div style="position:relative;overflow:hidden"><p id="p7">Lesson paragraph</p></div>',
      fragment: null, name: "",
    });
    backend.page_user_data = () => ({
      marks: [], notes: [], answers: {},
      bookmarks: [
        { slot: 0, blockType: 0, blockIdentifier: null },
        { slot: 1, blockType: 1, blockIdentifier: 7 },
      ],
    });
    const rects = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const top = this.matches(".reader") ? 100 : this.matches("h1") ? 300 : this.id === "p7" ? 500 : 0;
      return { top, bottom: top + 30, left: 0, right: 100, width: 100, height: 30, x: 0, y: top, toJSON: () => ({}) };
    });
    try {
      render(reader(documentTarget("nwtsty_E", 1)));
      await waitFor(() => expect(document.querySelectorAll(".bookmark-marker")).toHaveLength(2));
      const pageMarker = document.querySelector<HTMLElement>(".bookmark-page")!;
      const paragraphMarker = document.querySelector<HTMLElement>(".bookmark-marker[data-slot='1']")!.parentElement!;
      expect(pageMarker.style.top).toBe("200px");
      expect(paragraphMarker.style.top).toBe("400px");
      expect(pageMarker.parentElement?.classList.contains("reader")).toBe(true);
      expect(paragraphMarker.parentElement?.classList.contains("reader")).toBe(true);
      expect(document.querySelector("#p7")?.textContent).toBe("Lesson paragraph");
    } finally {
      rects.mockRestore();
    }
  });

  it("clears the clicked verse when navigating to another chapter", async () => {
    const view = render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await screen.findByLabelText("Show note");
    fireEvent.click(screen.getByText("For God loved the world"));
    view.rerender(reader(chapterTarget("nwtsty_E", 43, 4)));
    await screen.findByText("John 4");
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 1" }));
    await waitFor(() => expect(calls).toContainEqual({
      cmd: "save_bookmark",
      args: {
        target: chapterTarget("nwtsty_E", 43, 4),
        bookmark: { slot: 0, blockType: 0, blockIdentifier: null, snippet: null },
      },
    }));
  });

  it("saves the selected verse through the publication bookmark button", async () => {
    backend.publication_bookmarks = () => [];
    backend.save_bookmark = () => undefined;
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await screen.findByLabelText("Show note");
    const text = await screen.findByText("For God loved the world");
    const range = document.createRange();
    range.selectNodeContents(text);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 1" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "save_bookmark")).toBe(true));
    expect(calls.find((c) => c.cmd === "save_bookmark")?.args).toEqual({
      target: chapterTarget("nwtsty_E", 43, 3),
      bookmark: { slot: 0, blockType: 2, blockIdentifier: 16, snippet: "For God loved the world" },
    });
    selection.removeAllRanges();
  });

  it("scrolls to a bookmarked paragraph after loading its document", async () => {
    backend.render_page = () => ({
      title: "Article", html: '<p id="p1">Beginning</p><p id="p7">Bookmarked paragraph</p>',
      fragment: null, name: "",
    });
    const scrolling = vi.spyOn(Element.prototype, "scrollIntoView");
    render(
      <AppProvider>
        <ReaderView target={documentTarget("nwtsty_E", 1)} bookmark={{ blockType: 1, blockIdentifier: 7 }} />
      </AppProvider>,
    );
    await screen.findByText("Bookmarked paragraph");
    await waitFor(() => expect(scrolling.mock.instances.some((el) => (el as Element).id === "p7")).toBe(true));
    scrolling.mockRestore();
  });

  it("turns a lesson's goals into checkboxes, a date line and a text box that keep their answers", async () => {
    // A lesson's goal box, as the backend sends it: each field is a marker that names its kind.
    backend.render_page = () => ({
      title: "Lesson 1",
      html: `<label for="tt56" id="tt55">Lesson finished on: </label><span class="gen-tag gen-line" id="tt56"></span>
        <fieldset><legend>Goal</legend>
        <div class="gen-field" id="p43"><span class="gen-tag gen-check" id="tt62"></span><label for="tt62">Read the next lesson</label></div>
        <div class="gen-field" id="p44"><span class="gen-tag gen-check" id="tt66"></span><label for="tt66">Something else: </label>
        <div class="gen-field"><label for="tt71" class="dc-screenReaderText">Another goal</label><span class="gen-tag" id="tt71"></span></div></div>
        </fieldset>`,
      fragment: null,
      name: "",
    });
    backend.page_user_data = () => ({ marks: [], notes: [], answers: { tt56: "7 December", tt62: "1", tt71: "Visit" } });
    backend.save_answer = () => undefined;
    render(reader(documentTarget("lff_E", 3)));

    const done = await screen.findByRole<HTMLInputElement>("checkbox", { name: "Read the next lesson" });
    expect(done.checked).toBe(true);
    expect(screen.getByRole<HTMLInputElement>("checkbox", { name: "Something else:" }).checked).toBe(false);
    // The labels stay in the text next to their boxes.
    expect(screen.getByText("Read the next lesson").tagName).toBe("LABEL");
    expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Lesson finished on:" }).value).toBe("7 December");
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: "Another goal" }).value).toBe("Visit");

    fireEvent.click(done);
    await waitFor(() =>
      expect(calls).toContainEqual({ cmd: "save_answer", args: expect.objectContaining({ tag: "tt62", value: "0" }) }),
    );
  });

  it("edits a note in the study pane and moves its unsaved text to the note window when the chapter changes", async () => {
    const view = render(reader(chapterTarget("nwtsty_E", 43, 3)));
    fireEvent.click(await screen.findByText("Loved the world"));
    const editor = await screen.findByRole("form", { name: "Edit note" });
    const text = within(editor).getByPlaceholderText("Note");
    fireEvent.change(text, { target: { value: "God gave his only-begotten Son" } });

    // Keys typed in the note stay there and do not turn the page.
    const where = screen.getByTestId("where").textContent;
    fireEvent.keyDown(text, { key: "ArrowRight" });
    expect(screen.getByTestId("where").textContent).toBe(where);
    // The same key outside the note does turn the page.
    fireEvent.keyDown(document.body, { key: "ArrowRight" });
    await waitFor(() => expect(screen.getByTestId("where").textContent).toContain('"chapter":4'));

    view.rerender(reader(chapterTarget("nwtsty_E", 43, 4)));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_note_window")).toBe(true));
    await waitFor(() =>
      expect(vi.mocked(emitTo)).toHaveBeenCalledWith(
        "note",
        "note-open",
        expect.objectContaining({
          draft: expect.objectContaining({ note: expect.objectContaining({ guid: "n1" }) }),
          fields: { title: "Loved the world", content: "God gave his only-begotten Son", tags: "" },
        }),
      ),
    );
    await waitFor(() => expect(screen.queryByRole("form")).toBeNull());
  });

  it("follows a Bible link in a note like a cross-reference", async () => {
    backend.link_action = () => ({ kind: "open", target: chapterTarget("nwtsty_E", 43, 3), studyNote: false });
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    fireEvent.click(await screen.findByText("Loved the world"));
    const editor = await screen.findByRole("form", { name: "Edit note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value: "[Joh 3:16](pergament://bible/43:3:16-43:3:16)" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Preview" }));
    fireEvent.click(within(editor).getByText("Joh 3:16"));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "link_action")?.args).toEqual({ current: "nwtsty_E", href: "pergament://bible/43:3:16-43:3:16" }),
    );
    expect(calls.filter((c) => c.cmd === "link_action")).toHaveLength(1);
    expect(vi.mocked(emitTo)).not.toHaveBeenCalledWith("main", "note-link", expect.anything());
    expect(calls.some((c) => c.cmd === "open_external")).toBe(false);
  });

  /** Opens the note of John 3 in the study pane and changes its text without saving. */
  async function editNote(value: string) {
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    fireEvent.click(await screen.findByText("Loved the world"));
    const editor = await screen.findByRole("form", { name: "Edit note" });
    fireEvent.change(within(editor).getByPlaceholderText("Note"), { target: { value } });
    return editor;
  }
  /** Closes the reference and finds the note's text back in its editor. */
  async function backToNote(value: string) {
    fireEvent.click(await screen.findByRole("button", { name: "Back" }));
    const editor = await screen.findByRole("form", { name: "Edit note" });
    expect((within(editor).getByPlaceholderText("Note") as HTMLTextAreaElement).value).toBe(value);
    expect(calls.some((c) => c.cmd === "open_note_window")).toBe(false);
  }
  const elsewhere = () => ({ kind: "open", target: chapterTarget("nwtsty_E", 43, 4), studyNote: false });

  it("keeps the unsaved note behind a reference followed from its own preview", async () => {
    backend.link_action = elsewhere;
    const text = "Read [Joh 4:1](pergament://bible/43:4:1-43:4:1)";
    const editor = await editNote(text);
    fireEvent.click(within(editor).getByRole("button", { name: "Preview" }));
    fireEvent.click(within(editor).getByText("Joh 4:1"));
    expect(await screen.findByText("John 4")).toBeTruthy();
    expect(screen.queryByRole("form")).toBeNull();
    expect(calls.filter((c) => c.cmd === "link_action")).toHaveLength(1);
    await backToNote(text);
  });

  it("opens a link from the note window in the reference pane, keeping the page and the unsaved note", async () => {
    backend.link_action = elsewhere;
    await editNote("Unsaved thoughts");
    const where = screen.getByTestId("where").textContent;
    await waitFor(() => expect(listeners["note-link"]).toBeTruthy());
    await act(async () => listeners["note-link"]({ payload: "pergament://bible/43:4:1-43:4:1" }));
    expect(await screen.findByText("John 4")).toBeTruthy();
    expect(screen.getByTestId("where").textContent).toBe(where);
    expect(calls.filter((c) => c.cmd === "link_action")).toEqual([
      { cmd: "link_action", args: { current: "nwtsty_E", href: "pergament://bible/43:4:1-43:4:1" } },
    ]);
    await backToNote("Unsaved thoughts");
  });

  it("opens a link the note window linked and opened in the reference pane too", async () => {
    backend.link_action = elsewhere;
    render(reader(chapterTarget("nwtsty_E", 43, 3)));
    await screen.findByText("Loved the world");
    const where = screen.getByTestId("where").textContent;
    await waitFor(() => expect(listeners["note-show-link"]).toBeTruthy());
    await act(async () => listeners["note-show-link"]({ payload: "pergament://bible/43:4:1-43:4:1" }));
    expect(await screen.findByText("John 4")).toBeTruthy();
    expect(screen.getByTestId("where").textContent).toBe(where);
  });
});

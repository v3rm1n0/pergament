// @vitest-environment jsdom
// The reader on a phone-sized window: the study pane is a sheet that waits for a tap, and a desktop keeps it as a column.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { card } from "@/lib/fixtures";
import type { ChapterStudy, Note, Target } from "@/lib/api";
import { COMPACT } from "@/lib/viewport";

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
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
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

import { AppProvider } from "@/app";
import { ReaderView } from "@/views/reader";
import { chapterTarget } from "@/lib/api";

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

/** A chapter study with one footnote on verse 16, so the study has something to open on. */
const studyWithFootnote: ChapterStudy = {
  outlineTitle: null,
  outline: [],
  verses: [
    {
      chapter: 3,
      verse: 16,
      footnotes: [{ marker: "a", index: 1, html: "<p>Footnote text</p>" }],
      xrefs: [],
      notes: [],
    },
  ],
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
      return {
        title: `John ${c}`,
        html: `<p><span class="v" id="v43-${c}-16-1">For God loved the world</span> <a href="#footnote1">footnote</a></p>`,
        fragment: null,
        name: "",
      };
    },
    chapter_study: () => studyWithFootnote,
    research_verses: () => [],
    page_user_data: ({ target }: { target: Target }) => ({
      marks: [],
      notes: "chapter" in target.kind && target.kind.chapter.chapter === 3 ? [note] : [],
      answers: {},
    }),
    open_note_window: () => undefined,
  };
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** Answers the compact query with `compact`, and every other query with no match. */
function stubCompact(compact: boolean) {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: compact && query === COMPACT,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
}

const reader = (target: Target) => (
  <StrictMode>
    <AppProvider>
      <ReaderView target={target} />
    </AppProvider>
  </StrictMode>
);

/** Waits for the pane toggle, which is there once the study or the notes make the pane available. */
const paneToggle = () => screen.findByLabelText(/^(Show|Hide) study pane$/);

/** Mounts John 3 with its study loaded; the pane toggle appears once the study has arrived. */
async function renderReaderWithStudy() {
  render(reader(chapterTarget("nwtsty_E", 43, 3)));
  await paneToggle();
}

/** Mounts John 3, which has a note on it. */
async function renderReaderWithNotes() {
  render(reader(chapterTarget("nwtsty_E", 43, 3)));
  await paneToggle();
}

describe("reader on a phone", () => {
  it("does not cover a chapter with the study sheet on open", async () => {
    stubCompact(true);
    await renderReaderWithStudy();
    expect(screen.queryByLabelText("Study pane")).toBeNull();
    expect(screen.getByLabelText("Show study pane")).toBeTruthy();
  });

  it("stays closed when the chapter already has notes", async () => {
    stubCompact(true);
    await renderReaderWithNotes();
    expect(screen.queryByLabelText("Study pane")).toBeNull();
  });

  it("opens the sheet when a footnote is tapped", async () => {
    stubCompact(true);
    await renderReaderWithStudy();
    fireEvent.click(screen.getByRole("link", { name: /footnote/i }));
    expect(screen.getByLabelText("Study pane")).toBeTruthy();
  });

  it("keeps the column open by default on desktop", async () => {
    stubCompact(false);
    await renderReaderWithStudy();
    expect(screen.getByLabelText("Study pane")).toBeTruthy();
  });
});

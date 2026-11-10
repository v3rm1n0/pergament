// @vitest-environment jsdom
// Renders the Library, category and recording pages in StrictMode with a mocked backend and opens every tile menu.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { CatalogEntry, MediaCategory, MediaDownload, PubCard } from "@/lib/api";
import { card, entry, media } from "@/lib/fixtures";

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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined), emitTo: vi.fn() }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({ label: "main", onCloseRequested: async () => () => undefined }) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider, taskKey } from "@/app";
import { BibleView } from "@/views/bible";
import { ChaptersView } from "@/views/chapters";
import { CategoryView, LibraryView } from "@/views/library";
import { MediaCategoryView } from "@/views/media";

let publications: PubCard[];
let items: CatalogEntry[];
let recordings: MediaDownload[];

const download = (over: Partial<MediaDownload> = {}): MediaDownload => ({
  id: 1,
  key: "pub-nwtsv_X_1_VIDEO",
  langCode: "X",
  title: "Downloaded video",
  kind: "video",
  label: "480p",
  size: 1e6,
  duration: 60,
  image: null,
  path: "/media/a.mp4",
  subtitlePath: null,
  downloadedAt: 0,
  ...over,
});

beforeEach(() => {
  calls.length = 0;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  publications = [card({ dir: "lff_2", symbol: "lff", title: "Glücklich" })];
  items = [entry("lff", "Brochures and Booklets", "lff_2", "Glücklich"), entry("ll", "Brochures and Booklets", null, "Höre auf Gott")];
  recordings = [download()];
  const category: MediaCategory = {
    key: "VOD",
    name: "Videos",
    container: false,
    image: null,
    subcategories: [],
    media: [media("pub-nwtsv_X_1_VIDEO", "A video"), media("pub-x_X_2_VIDEO", "Another video")],
  };
  backend = {
    favorites: () => [],
    list_publications: () => publications,
    languages: () => [
      { code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false },
      { code: "E", name: "English", vernacular: "English", rtl: false, signLanguage: false },
    ],
    catalog_cached: () => true,
    check_updates: () => null,
    categories: () => [{ id: 4, name: "Brochures and Booklets", count: 2 }],
    category: () => items,
    media_category: () => category,
    media_downloads: () => recordings,
    remove_publication: ({ dir }: { dir: string }) => {
      publications = publications.filter((p) => p.dir !== dir);
    },
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

describe("Category page", () => {
  it("gives every grid card a menu, with Remove only for downloaded ones", async () => {
    renderIn(<CategoryView id={4} title="Brochures" />);
    openMenu(await tile("Höre auf Gott"));
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.queryByText("Remove")).toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    openMenu(await tile("Glücklich"));
    expect(screen.getByText("Remove")).toBeTruthy();
  });

  it("copies a link built from the entry", async () => {
    renderIn(<CategoryView id={4} title="Brochures" />);
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith("https://www.jw.org/finder?pub=ll&wtlocale=X"),
    );
  });

  it("lists languages from the catalog and downloads a missing one without opening it", async () => {
    backend.catalog_languages = () => [
      { code: "X", entry: entry("ll", "Brochures and Booklets", null, "Höre auf Gott") },
      { code: "E", entry: entry("ll", "Brochures and Booklets", null, "Listen to God") },
    ];
    backend.download_publication = () => "ll_0";
    renderIn(<CategoryView id={4} title="Brochures" />);
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("More Languages"));
    fireEvent.click(await screen.findByText("English"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("shows a hint instead of an empty list when no catalog is cached", async () => {
    backend.catalog_languages = () => null;
    renderIn(<CategoryView id={4} title="Brochures" />);
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("More Languages"));
    expect(await screen.findByText("Load the catalog to see more languages.")).toBeTruthy();
  });

  it("copes with a catalog that lists the publication in no other language", async () => {
    backend.catalog_languages = () => [];
    renderIn(<CategoryView id={4} title="Brochures" />);
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("More Languages"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "catalog_languages")).toBe(true));
    expect(screen.queryByText("Loading…")).toBeNull();
  });
});

describe("Downloaded tab", () => {
  it("uses the key symbol for both the Library star and menu", async () => {
    const favorite = { keySymbol: "w", mepsLanguage: 2, issueTag: 20260800 };
    let saved = [favorite];
    publications = [card({ dir: "unrelated-directory", symbol: "w26", keySymbol: "w", issueTag: "20260800", title: "Downloaded issue" })];
    backend.favorites = () => saved;
    backend.set_favorite = ({ key, favorite: on }) => {
      expect(key).toEqual(favorite);
      saved = on ? [favorite] : [];
    };
    renderIn(<LibraryView tab="downloaded" />);
    const t = await tile("Downloaded issue");
    expect(within(t).getByLabelText("Remove from favorites").getAttribute("aria-pressed")).toBe("true");
    openMenu(t);
    fireEvent.click(screen.getByText("Remove from Favorites"));
    await waitFor(() => expect(within(t).getByLabelText("Add to favorites").getAttribute("aria-pressed")).toBe("false"));
    fireEvent.click(within(t).getByLabelText("Add to favorites"));
    await waitFor(() => expect(saved).toEqual([favorite]));
    openMenu(t);
    expect(screen.getByText("Remove from Favorites")).toBeTruthy();
  });

  it("removes a publication after confirming, from its menu", async () => {
    renderIn(<LibraryView tab="downloaded" />);
    openMenu(await tile("Glücklich"));
    fireEvent.click(screen.getByText("Remove"));
    expect(calls.some((c) => c.cmd === "remove_publication")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "remove_publication")).toBe(true));
    await waitFor(() => expect(screen.queryByText("Glücklich")).toBeNull());
  });

  it("cancels the removal", async () => {
    renderIn(<LibraryView tab="downloaded" />);
    openMenu(await tile("Glücklich"));
    fireEvent.click(screen.getByText("Remove"));
    fireEvent.click(screen.getByText("Cancel"));
    expect(calls.some((c) => c.cmd === "remove_publication")).toBe(false);
    expect(screen.getByText("Glücklich")).toBeTruthy();
  });

  it("gives downloaded recordings a menu with a link without the language", async () => {
    renderIn(<LibraryView tab="downloaded" />);
    const t = (await screen.findByText("Downloaded video")).closest(".group") as HTMLElement;
    openMenu(t);
    expect(screen.queryByText("More Languages")).toBeNull();
    expect(screen.queryByText("Add to Favorites")).toBeNull();
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() =>
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
        "https://www.jw.org/finder?lank=pub-nwtsv_1_VIDEO&wtlocale=X",
      ),
    );
  });
});

describe("Recording pages", () => {
  it("gives video cards a menu with only Share Link, and the cloud menu still works", async () => {
    renderIn(<MediaCategoryView catKey="VOD" title="Videos" />);
    openMenu(await tile("A video"));
    expect(screen.getByText("Share Link")).toBeTruthy();
    expect(screen.queryByText("More Languages")).toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    const cloud = within(await tile("Another video")).getByLabelText("Download");
    fireEvent.pointerDown(cloud, { button: 0, ctrlKey: false });
    expect(screen.getByText(/Download 480p/)).toBeTruthy();
  });

  it("gives audio rows a menu", async () => {
    const audio: MediaCategory = {
      key: "AUD",
      name: "Audio",
      container: false,
      image: null,
      subcategories: [],
      media: [media("pub-a_X_1_AUDIO", "A song", "audio")],
    };
    backend.media_category = () => audio;
    renderIn(<MediaCategoryView catKey="AUD" title="Audio" />);
    openMenu(await tile("A song"));
    expect(screen.getByText("Share Link")).toBeTruthy();
  });
});

describe("Coming back to a page", () => {
  it("shows the library categories right away", async () => {
    const first = renderIn(<LibraryView />);
    await screen.findByText("Brochures and Booklets");
    first.unmount();
    renderIn(<LibraryView />);
    expect(screen.getByText("Brochures and Booklets")).toBeTruthy();
  });

  it("shows the items of a category right away", async () => {
    const first = renderIn(<CategoryView id={4} title="Brochures" />);
    await screen.findByText("Höre auf Gott");
    first.unmount();
    renderIn(<CategoryView id={4} title="Brochures" />);
    expect(screen.getByText("Höre auf Gott")).toBeTruthy();
  });

  it("shows the recordings of a category right away", async () => {
    const first = renderIn(<MediaCategoryView catKey="VOD" title="Videos" />);
    await screen.findByText("A video");
    first.unmount();
    renderIn(<MediaCategoryView catKey="VOD" title="Videos" />);
    expect(screen.getByText("A video")).toBeTruthy();
  });

  it("shows the catalog prompt when a category fails to load", async () => {
    backend.category = () => {
      throw new Error("boom");
    };
    renderIn(<CategoryView id={5} title="Books" />);
    await waitFor(() => expect(screen.queryByPlaceholderText("Filter")).toBeNull());
  });
});

describe("Bibles", () => {
  it("are not a Library category, since the Bible tab downloads them", async () => {
    backend.categories = () => [
      { id: 1, name: "Bible", count: 3 },
      { id: 4, name: "Brochures and Booklets", count: 2 },
    ];
    publications = [card({ dir: "nwt_2", symbol: "nwt", title: "Bibel", publicationType: "Bible", isBible: true })];
    renderIn(<LibraryView />);
    await screen.findByText("Brochures and Booklets");
    expect(screen.queryByText("Bible")).toBeNull();
  });

  it("are listed by the Bible tab for download when the language has none", async () => {
    items = [entry("nwtsty", "Bible", null, "Studienausgabe"), entry("nwt", "Bible", null, "Neue-Welt-Übersetzung")];
    publications = [];
    renderIn(<BibleView />);
    expect(await screen.findByText("Studienausgabe")).toBeTruthy();
    expect(screen.getByText("Neue-Welt-Übersetzung")).toBeTruthy();
    expect(calls.some((c) => c.cmd === "category" && (c.args as { id: number }).id === 1)).toBe(true);
  });
});

describe("Bible downloads", () => {
  it("show their progress on the translation while they run", async () => {
    items = [entry("nwtsty", "Bible", null, "Studienausgabe"), entry("nwt", "Bible", null, "Neue-Welt-Übersetzung")];
    publications = [];
    backend.download_publication = () => new Promise(() => undefined);
    renderIn(<BibleView />);
    fireEvent.click(await screen.findByText("Studienausgabe"));
    const bar = await screen.findByRole("progressbar", { name: "Studienausgabe" });
    expect(bar).toBeTruthy();
    expect(screen.getByText("0%")).toBeTruthy();
    expect(screen.queryByRole("progressbar", { name: "Neue-Welt-Übersetzung" })).toBeNull();
  });
});

describe("Chapters of a Bible book", () => {
  it("shows the overview page in the study pane, and opens a chapter from its links", async () => {
    const bible = card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Studienbibel", isBible: true });
    backend.publication = () => ({
      card: bible,
      toc: [],
      books: [{ number: 40, title: "Das Evangelium nach Matthäus", chapter_title: "Matthäus", book_document_id: 145, chapters: 28 }],
    });
    const video = { pub_symbol: "nwtsv", issue: null, track: 400, kind: "video", lang_code: "X" };
    backend.book_overview = () => ({ document: 144, video });
    backend.media_links = () => [];
    backend.render_page = () => ({
      title: "Übersicht über Matthäus",
      fragment: null,
      name: "x",
      html: `<header><h1><strong>Übersicht über Matthäus</strong></h1></header><ul class="outline"><li class="L1"><p><span class="itemLabel"><strong>A.</strong> </span><strong>Abstammung (</strong><a href="pergament://verse/X:1/1:1-1:17"><strong>1:1-17</strong></a><strong>)</strong></p></li></ul>`,
    });
    backend.link_action = () => ({
      kind: "open",
      target: { publication: "nwtsty_2", kind: { chapter: { book: 40, chapter: 1, verse: 1 } } },
      studyNote: false,
    });
    renderIn(<ChaptersView dir="nwtsty_2" book={40} />);
    const pane = await screen.findByLabelText("Study pane");
    expect(within(pane).getByText("Übersicht über Matthäus")).toBeTruthy();
    fireEvent.click(screen.getByText("1:1-17"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "link_action")).toBe(true));
    expect(screen.getByText("28")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Introduction video/ }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "media_links" && (c.args as { media: { track: number } }).media.track === 400)).toBe(true));
  });

  it("follows external and missing links in the overview instead of ignoring them", async () => {
    backend.publication = () => ({
      card: card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Studienbibel", isBible: true }),
      toc: [],
      books: [{ number: 40, title: "Matthäus", chapter_title: "Matthäus", book_document_id: 145, chapters: 28 }],
    });
    backend.book_overview = () => ({ document: 144, video: null });
    backend.render_page = () => ({
      title: "Übersicht",
      fragment: null,
      name: "x",
      html: `<p><a href="https://example.test/a">Außen</a> <a href="jwpub://p/X:9/">Fehlt</a></p>`,
    });
    backend.open_external = () => undefined;
    backend.link_action = ({ href }: { href: string }) =>
      href.startsWith("https") ? { kind: "external", url: href } : { kind: "missing", url: "jw.org/x" };
    renderIn(<ChaptersView dir="nwtsty_2" book={40} />);
    fireEvent.click(await screen.findByText("Außen"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "open_external")).toBe(true));
    fireEvent.click(screen.getByText("Fehlt"));
    expect(await screen.findByText(/Not in your library: jw.org\/x/)).toBeTruthy();
  });

  it("has no video button for a book without an introduction video", async () => {
    backend.publication = () => ({
      card: card({ dir: "nwtsty_2", symbol: "nwtsty", title: "Studienbibel", isBible: true }),
      toc: [],
      books: [{ number: 66, title: "Offenbarung", chapter_title: "Offenbarung", book_document_id: 1, chapters: 22 }],
    });
    backend.book_overview = () => ({ document: null, video: null });
    renderIn(<ChaptersView dir="nwtsty_2" book={66} />);
    expect(await screen.findByText("22")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /Introduction video/ })).toBeNull();
  });
});

describe("download progress", () => {
  it("keeps the same publication in two languages apart", () => {
    const de = entry("Rbi8", "Bible", null);
    const en = { ...de, item: { ...de.item, meps_language: 0 } };
    expect(taskKey(en)).not.toBe(taskKey(de));
    expect(taskKey(de)).toBe("download:2:Rbi8:0");
  });
});

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
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({ label: "main" }) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider } from "@/app";
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
const openMenu = (t: HTMLElement) => fireEvent.click(within(t).getByLabelText("More"));

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
    fireEvent.click(cloud);
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

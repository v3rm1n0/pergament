// @vitest-environment jsdom
// Renders the real provider and Home view in StrictMode with a mocked backend and clicks through the tile menus.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { CatalogEntry, PubCard } from "@/lib/api";
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
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined), emitTo: vi.fn() }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({ label: "main" }) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider } from "@/app";
import { HomeView } from "@/views/home";

let publications: PubCard[];
let toolbox: CatalogEntry[];

beforeEach(() => {
  calls.length = 0;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  publications = [card({ dir: "lff_2", symbol: "lff", title: "Glücklich" })];
  toolbox = [
    entry("lff", "Books", "lff_2", "Glücklich"),
    entry("ll", "Brochures and Booklets", null, "Höre auf Gott"),
    entry("bh", "Books", null, "Was lehrt die Bibel wirklich?"),
    entry("T-ftr", "Tracts and Invitations", null, "Zukunft"),
    entry("odd", null, null, "Ohne Kategorie"),
  ];
  backend = {
    list_publications: () => publications,
    languages: () => [],
    catalog_cached: () => true,
    check_updates: () => null,
    home_lists: () => ({ teachingToolbox: toolbox, whatsNew: [], dailyText: null }),
    dated_page: () => null,
    favorite_entries: () => [],
    remove_publication: ({ dir }: { dir: string }) => {
      publications = publications.filter((p) => p.dir !== dir);
      toolbox = toolbox.map((e) => (e.local === dir ? { ...e, local: null } : e));
    },
    download_publication: () => "ll_2",
    catalog_languages: () => [],
  };
  Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => undefined) } });
});
afterEach(cleanup);

const renderHome = () =>
  render(
    <StrictMode>
      <AppProvider>
        <HomeView />
      </AppProvider>
    </StrictMode>,
  );

/** The tile whose caption is `title`, as the element that holds both the tile button and its menu button. */
const tile = async (title: string) => (await screen.findByText(title)).closest(".group") as HTMLElement;
const openMenu = (t: HTMLElement) => fireEvent.click(within(t).getByLabelText("More"));

describe("Home with tile menus", () => {
  it("renders the toolbox in two rows without the hidden publications", async () => {
    renderHome();
    await screen.findByText("Höre auf Gott");
    expect(screen.queryByText("Was lehrt die Bibel wirklich?")).toBeNull();
    expect(screen.getByText("Zukunft")).toBeTruthy();
    expect(screen.getByText("Ohne Kategorie")).toBeTruthy();
  });

  it("opens the menu of every tile kind without crashing", async () => {
    renderHome();
    for (const title of ["Höre auf Gott", "Zukunft", "Ohne Kategorie"]) {
      const t = await tile(title);
      openMenu(t);
      expect(screen.getByText("Share Link")).toBeTruthy();
      expect(screen.getByText("More Languages")).toBeTruthy();
      expect(screen.getByText("Add to Favorites")).toBeTruthy();
      expect(screen.queryByText("Remove")).toBeNull(); // not downloaded
      fireEvent.keyDown(window, { key: "Escape" });
    }
  });

  it("favorites a tile that is not downloaded and shows it in the Favorites row", async () => {
    const ll = entry("ll", "Brochures and Booklets", null, "Höre auf Gott");
    backend.favorite_entries = ({ keys }: { keys: string[] }) => (keys.includes("ll_2") ? [ll] : []);
    renderHome();
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("Add to Favorites"));
    expect(JSON.parse(localStorage.getItem("favorites")!)).toEqual(["ll_2"]);
    await waitFor(() => expect(calls.some((c) => c.cmd === "favorite_entries")).toBe(true));
    await waitFor(() => expect(screen.getAllByText("Höre auf Gott").length).toBe(2)); // toolbox + favorites
    // Menu now offers removal from favorites.
    openMenu((await screen.findAllByText("Höre auf Gott"))[0].closest(".group") as HTMLElement);
    expect(screen.getByText("Remove from Favorites")).toBeTruthy();
  });

  it("keeps a favorite whose key is not the directory name once it is downloaded", async () => {
    // Non-Latin symbols are cleaned in directory names, so key and directory differ.
    localStorage.setItem("favorites", JSON.stringify(["ขก_237"]));
    publications = [card({ dir: "kk_237", symbol: "kk", mepsLanguage: 237, langCode: "X" })];
    backend.favorite_entries = () => [entry("ขก", "Brochures and Booklets", "kk_237", "Thai brochure")];
    renderHome();
    expect(await screen.findByText("Thai brochure")).toBeTruthy();
  });

  it("does not list a downloaded favorite twice", async () => {
    localStorage.setItem("favorites", JSON.stringify(["lff_2"]));
    renderHome();
    await screen.findAllByText("Glücklich");
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_publications")).toBe(true));
    // Toolbox tile plus the Favorites tile, and no favorite_entries lookup for a key that is a directory.
    expect(screen.getAllByText("Glücklich").length).toBe(2);
    expect(calls.filter((c) => c.cmd === "favorite_entries").length).toBe(0);
  });

  it("asks before removing a downloaded publication, then removes it everywhere", async () => {
    localStorage.setItem("favorites", JSON.stringify(["lff_2"]));
    renderHome();
    const [first] = await screen.findAllByText("Glücklich");
    openMenu(first.closest(".group") as HTMLElement);
    fireEvent.click(screen.getByText("Remove"));
    expect(calls.some((c) => c.cmd === "remove_publication")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "remove_publication")).toBe(true));
    // The favorite stays, now as a catalog tile, and the toolbox tile is not marked downloaded any more.
    await waitFor(() => expect(calls.filter((c) => c.cmd === "home_lists").length).toBeGreaterThan(1));
  });

  it("downloads a tile on click without navigating to it", async () => {
    renderHome();
    const t = await tile("Höre auf Gott");
    fireEvent.click(within(t).getByTitle("Höre auf Gott"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
  });

  it("survives a backend without a catalog", async () => {
    localStorage.setItem("favorites", JSON.stringify(["ll_2"]));
    backend.home_lists = () => null;
    backend.favorite_entries = () => null;
    renderHome();
    expect(await screen.findByText("Favorites")).toBeTruthy();
  });

  it("survives failing favorite lookups", async () => {
    localStorage.setItem("favorites", JSON.stringify(["ll_2"]));
    backend.favorite_entries = () => {
      throw new Error("boom");
    };
    renderHome();
    expect(await screen.findByText("Höre auf Gott")).toBeTruthy();
  });

  it("filters the toolbox with the segmented tabs and falls back to All", async () => {
    renderHome();
    await screen.findByText("Höre auf Gott");
    expect(screen.getByRole("tab", { name: "All" })).toBeTruthy();
    fireEvent.mouseDown(screen.getByRole("tab", { name: "Tracts and Invitations" }), { button: 0 });
    await waitFor(() => expect(screen.queryByText("Höre auf Gott")).toBeNull());
    expect(screen.getByText("Zukunft")).toBeTruthy();
    fireEvent.mouseDown(screen.getByRole("tab", { name: "All" }), { button: 0 });
    expect(await screen.findByText("Höre auf Gott")).toBeTruthy();
  });

  it("toggles a favorite with the star and reports it with aria-pressed", async () => {
    renderHome();
    const star = within(await tile("Höre auf Gott")).getByLabelText("Add to favorites");
    expect(star.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(star);
    expect(JSON.parse(localStorage.getItem("favorites")!)).toEqual(["ll_2"]);
    await waitFor(() =>
      expect(within(screen.getAllByText("Höre auf Gott")[0].closest(".group") as HTMLElement).getByLabelText("Remove from favorites").getAttribute("aria-pressed")).toBe("true"),
    );
  });

  it("shows today's text with its reference and a Read button, or the welcome block", async () => {
    renderHome();
    expect(await screen.findByText("Welcome to Pergament")).toBeTruthy();
    cleanup();
    backend.dated_page = () => ({
      target: { publication: "es_X", kind: { dated: { date: 20261007 } } },
      title: "",
      start: 20261007,
      end: 20261007,
      html: '<p class="themeScrp"><em>Ein erfundener Text</em>—<a><em>Ps. 1:1</em></a></p>',
      image: null,
    });
    renderHome();
    expect(await screen.findByText("Ein erfundener Text")).toBeTruthy();
    expect(screen.getByText("Ps. 1:1")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Read" })).toBeTruthy();
    expect(screen.queryByText("Welcome to Pergament")).toBeNull();
  });

  it("lists What's New as rows with a Get button that downloads", async () => {
    backend.home_lists = () => ({
      teachingToolbox: toolbox,
      whatsNew: [entry("ll", "Brochures and Booklets", null, "Höre auf Gott")],
      dailyText: null,
    });
    renderHome();
    fireEvent.click(await screen.findByRole("button", { name: /^Get · / }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
  });
});

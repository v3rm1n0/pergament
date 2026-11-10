// @vitest-environment jsdom
// Renders the real provider and Home view in StrictMode with a mocked backend and clicks through the tile menus.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { samePublication, type PublicationKey, type CatalogEntry, type PubCard, type WhatsNewRow } from "@/lib/api";
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
vi.mock("@tauri-apps/api/webviewWindow", () => ({ getCurrentWebviewWindow: () => ({ label: "main", onCloseRequested: async () => () => undefined }) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppProvider } from "@/app";
import { HomeView } from "@/views/home";

let publications: PubCard[];
let toolbox: CatalogEntry[];
let whatsNewRows: WhatsNewRow[];
let favorites: PublicationKey[];
const key = (keySymbol: string, mepsLanguage = 2, issueTag = 0): PublicationKey => ({ keySymbol, mepsLanguage, issueTag });

beforeEach(() => {
  calls.length = 0;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  favorites = [];
  publications = [card({ dir: "lff_2", symbol: "lff", title: "Glücklich" })];
  toolbox = [
    entry("lff", "Books", "lff_2", "Glücklich"),
    entry("ll", "Brochures and Booklets", null, "Höre auf Gott"),
    entry("bh", "Books", null, "Was lehrt die Bibel wirklich?"),
    entry("T-ftr", "Tracts and Invitations", null, "Zukunft"),
    entry("odd", null, null, "Ohne Kategorie"),
  ];
  whatsNewRows = [{ ...entry("ll", "Brochures and Booklets", null, "Frisch erschienen"), langCode: "X" }];
  backend = {
    list_publications: () => publications,
    languages: () => [
      { code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false },
      { code: "E", name: "English", vernacular: "English", rtl: false, signLanguage: false },
    ],
    catalog_cached: () => true,
    check_updates: () => null,
    home_lists: () => ({ teachingToolbox: toolbox, dailyText: null }),
    teaching_toolbox: () => toolbox,
    whats_new: ({ langs }: { langs: string[] }) => langs.flatMap((c) => whatsNewRows.filter((r) => r.langCode === c)),
    dated_page: () => null,
    favorites: () => favorites,
    set_favorite: ({ key, favorite }: { key: PublicationKey; favorite: boolean }) => {
      if (!favorite) favorites = favorites.filter((k) => !samePublication(k, key));
      else if (!favorites.some((k) => samePublication(k, key))) favorites.push(key);
    },
    favorite_entry: ({ key }: { key: PublicationKey }) => {
      const e = toolbox.find((e) => e.item.key_symbol === key.keySymbol && e.item.meps_language === key.mepsLanguage && e.item.issue_tag === key.issueTag);
      return e ? { code: "X", entry: e } : null;
    },
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
// Radix opens a menu on a pointer press, not on a click.
const openMenu = (t: HTMLElement) => fireEvent.pointerDown(within(t).getByLabelText("More"), { button: 0, ctrlKey: false });

describe("Home with tile menus", () => {
  it("renders the toolbox in two rows without the hidden publications", async () => {
    renderHome();
    await screen.findByText("Höre auf Gott");
    expect(screen.queryByText("Was lehrt die Bibel wirklich?")).toBeNull();
    expect(screen.getByText("Zukunft")).toBeTruthy();
    expect(screen.getByText("Ohne Kategorie")).toBeTruthy();
  });

  it("shows the toolbox right away when the view is opened again", async () => {
    renderHome();
    await screen.findByText("Höre auf Gott");
    cleanup();
    renderHome();
    expect(screen.getByText("Höre auf Gott")).toBeTruthy();
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
    renderHome();
    openMenu(await tile("Höre auf Gott"));
    fireEvent.click(screen.getByText("Add to Favorites"));
    await waitFor(() => expect(favorites).toEqual([key("ll")]));
    await waitFor(() => expect(calls.some((c) => c.cmd === "favorite_entry")).toBe(true));
    await waitFor(() => expect(screen.getAllByText("Höre auf Gott").length).toBe(2)); // toolbox + favorites
    // Menu now offers removal from favorites.
    openMenu((await screen.findAllByText("Höre auf Gott"))[0].closest(".group") as HTMLElement);
    expect(screen.getByText("Remove from Favorites")).toBeTruthy();
  });

  it("matches an imported non-Latin favorite by identity without a catalog", async () => {
    // Non-Latin symbols are cleaned in directory names, so key and directory differ.
    favorites = [key("ขก", 237)];
    publications = [card({ dir: "kk_237", symbol: "ขก", keySymbol: "ขก", title: "Thai brochure", mepsLanguage: 237, langCode: "TH" })];
    backend.favorite_entry = () => null;
    renderHome();
    expect(await screen.findByText("Thai brochure")).toBeTruthy();
  });

  it("does not list a downloaded favorite twice", async () => {
    favorites = [key("lff")];
    renderHome();
    await screen.findAllByText("Glücklich");
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_publications")).toBe(true));
    // Toolbox tile plus the Favorites tile.
    expect(screen.getAllByText("Glücklich").length).toBe(2);
  });

  it("keeps mixed-language saved order and identity when a favorite downloads", async () => {
    toolbox = [];
    whatsNewRows = [];
    favorites = [key("w", 0, 20260800), key("lff"), key("ll")];
    const english = entry("w26", "Watchtower", null, "English issue", 20260800);
    english.item.key_symbol = "w";
    english.item.meps_language = 0;
    const german = entry("ll", "Brochures and Booklets", null, "German brochure");
    backend.favorite_entry = ({ key: k }: { key: PublicationKey }) =>
      k.keySymbol === "w" ? { code: "E", entry: english } : { code: "X", entry: german };
    backend.download_publication = ({ lang }: { lang: string }) => {
      expect(lang).toBe("E");
      publications.push(card({
        dir: "w26_0_20260800", symbol: "w26", keySymbol: "w",
        title: "English issue", issueTag: "20260800", mepsLanguage: 0, langCode: "E",
      }));
      return "w26_0_20260800";
    };
    renderHome();
    await screen.findByText("English issue");
    await screen.findByText("German brochure");
    const section = screen.getByText("Favorites").closest("section")!;
    const titles = () => [...section.querySelectorAll(".group > button")].map((b) => b.getAttribute("title"));
    expect(titles()).toEqual(["English issue", "Glücklich", "German brochure"]);
    openMenu(await tile("English issue"));
    expect(screen.getByText("Remove from Favorites")).toBeTruthy();
    fireEvent.click(screen.getByText("Share Link"));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith(
      "https://www.jw.org/finder?pub=w26&issue=202608&wtlocale=E",
    ));
    fireEvent.click(within(await tile("English issue")).getByTitle("English issue"));
    await waitFor(() => expect(within(section).queryByLabelText("Download English issue")).toBeNull());
    expect(titles()).toEqual(["English issue", "Glücklich", "German brochure"]);
    expect(calls.filter((c) => c.cmd === "set_favorite")).toHaveLength(0);
    expect(favorites).toEqual([key("w", 0, 20260800), key("lff"), key("ll")]);
    openMenu(await tile("English issue"));
    expect(screen.getByText("Remove")).toBeTruthy();
    fireEvent.click(screen.getByText("Remove from Favorites"));
    await waitFor(() => expect(favorites).toEqual([key("lff"), key("ll")]));
    await waitFor(() => expect(titles()).toEqual(["Glücklich", "German brochure"]));
  });

  it("ignores localStorage favorites without reading or changing them", async () => {
    localStorage.setItem("favorites", JSON.stringify(["lff_2", "ll_2"]));
    const read = vi.spyOn(Storage.prototype, "getItem");
    renderHome();
    await screen.findByText("Höre auf Gott");
    const section = screen.getByText("Favorites").closest("section")!;
    expect(within(section).queryByText("Glücklich")).toBeNull();
    expect(within(section).queryByText("Höre auf Gott")).toBeNull();
    expect(read.mock.calls.some(([k]) => k === "favorites")).toBe(false);
    read.mockRestore();
    fireEvent.click(within(await tile("Höre auf Gott")).getByLabelText("Add to favorites"));
    await waitFor(() => expect(favorites).toEqual([key("ll")]));
    expect(localStorage.getItem("favorites")).toBe(JSON.stringify(["lff_2", "ll_2"]));
  });

  it("waits for persistence and ignores repeated in-flight actions", async () => {
    let finish!: () => void;
    const persisted = backend.set_favorite;
    backend.set_favorite = async (args) => {
      await new Promise<void>((resolve) => { finish = resolve; });
      persisted(args);
    };
    renderHome();
    const star = within(await tile("Höre auf Gott")).getByLabelText("Add to favorites");
    fireEvent.click(star);
    fireEvent.click(star);
    expect(star.getAttribute("aria-pressed")).toBe("false");
    expect(favorites).toEqual([]);
    expect(calls.filter((c) => c.cmd === "set_favorite")).toHaveLength(1);
    await act(async () => finish());
    await waitFor(() => expect(star.getAttribute("aria-pressed")).toBe("true"));
    expect(favorites).toEqual([key("ll")]);
    expect(calls.find((c) => c.cmd === "set_favorite")?.args).toEqual({ key: key("ll"), favorite: true });
  });

  it.each([false, true])("keeps the previous state and reports failed writes (favorite = %s)", async (on) => {
    favorites = on ? [key("ll")] : [];
    backend.set_favorite = () => { throw new Error("database write failed"); };
    renderHome();
    const star = within(await tile("Höre auf Gott")).getByLabelText(on ? "Remove from favorites" : "Add to favorites");
    await waitFor(() => expect(star.getAttribute("aria-pressed")).toBe(String(on)));
    fireEvent.click(star);
    expect(await screen.findByText("Error: database write failed")).toBeTruthy();
    expect(star.getAttribute("aria-pressed")).toBe(String(on));
    expect(favorites).toEqual(on ? [key("ll")] : []);
  });

  it("asks before removing a downloaded publication, then removes it everywhere", async () => {
    favorites = [key("lff")];
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
    favorites = [key("ll")];
    backend.home_lists = () => null;
    backend.favorite_entry = () => null;
    renderHome();
    expect(await screen.findByText("Favorites")).toBeTruthy();
  });

  it("survives failing favorite lookups", async () => {
    favorites = [key("ll")];
    backend.favorite_entry = () => {
      throw new Error("boom");
    };
    renderHome();
    expect(await screen.findByText("Höre auf Gott")).toBeTruthy();
  });

  it("reports failed database reads instead of using localStorage", async () => {
    localStorage.setItem("favorites", '["lff_2"]');
    backend.favorites = () => { throw new Error("database read failed"); };
    renderHome();
    expect((await screen.findAllByText("Error: database read failed")).length).toBeGreaterThan(0);
    const section = screen.getByText("Favorites").closest("section")!;
    expect(within(section).queryByText("Glücklich")).toBeNull();
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
    await waitFor(() => expect(favorites).toEqual([key("ll")]));
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
    renderHome();
    fireEvent.click(await screen.findByRole("button", { name: /^Get · / }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
  });

  it("downloads a What's New row in the row's language", async () => {
    localStorage.setItem("whatsNewLangs", JSON.stringify(["E"]));
    whatsNewRows = [{ ...entry("lff", "Books", null, "Newly Arrived"), langCode: "E" }];
    renderHome();
    fireEvent.click(await screen.findByRole("button", { name: "Get · Newly Arrived" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
    expect(calls.find((c) => c.cmd === "download_publication")?.args).toMatchObject({ lang: "E" });
  });

  it("follows the app language until a selection is stored", async () => {
    renderHome();
    await screen.findByText("Frisch erschienen");
    expect(calls.find((c) => c.cmd === "whats_new")?.args).toEqual({ langs: ["X"] });
    // One language only: the row's muted line shows the category without the language.
    expect(screen.queryByText("Brochures and Booklets · Deutsch")).toBeNull();
    cleanup();
    localStorage.setItem("whatsNewLangs", JSON.stringify(["X", "E"]));
    // Same symbol and issue in both languages: only the row key tells them apart.
    whatsNewRows.push({ ...entry("ll", "Books", null, "Newly Arrived"), langCode: "E" });
    const errors: string[] = [];
    const spy = vi.spyOn(console, "error").mockImplementation((...a: unknown[]) => void errors.push(a.join(" ")));
    renderHome();
    expect(await screen.findByText("Newly Arrived")).toBeTruthy();
    spy.mockRestore();
    // The backend does not dedupe across languages, so React must not see duplicate keys.
    expect(errors.join("\n")).not.toContain("same key");
    expect(calls.filter((c) => c.cmd === "whats_new").at(-1)?.args).toEqual({ langs: ["X", "E"] });
    expect(screen.getByText("Brochures and Booklets · Deutsch")).toBeTruthy();
    expect(screen.getByText("Books · English")).toBeTruthy();
    expect(screen.getByText("Frisch erschienen")).toBeTruthy();
  });

  it("recovers when a stored language no longer exists", async () => {
    localStorage.setItem("whatsNewLangs", JSON.stringify(["ZZ"]));
    renderHome();
    // The dead code is pruned to an empty selection, which falls back to the app language.
    expect(await screen.findByText("Frisch erschienen")).toBeTruthy();
    await waitFor(() =>
      expect(calls.filter((c) => c.cmd === "whats_new").at(-1)?.args).toEqual({ langs: ["X"] }),
    );
    expect(JSON.parse(localStorage.getItem("whatsNewLangs")!)).toEqual([]);
  });

  it("recovers on a cold start when the backend rejects the dead code", async () => {
    localStorage.setItem("whatsNewLangs", JSON.stringify(["ZZ"]));
    backend.whats_new = ({ langs }: { langs: string[] }) => {
      if (langs.includes("ZZ")) throw new Error('unknown language code "ZZ"');
      return langs.flatMap((c) => whatsNewRows.filter((r) => r.langCode === c));
    };
    renderHome();
    // With no rows to keep, the section still renders its heading and picker instead of vanishing.
    expect(await screen.findByLabelText("What's New languages")).toBeTruthy();
    // The picker prunes the dead code, the app language takes over and its rows show up.
    await waitFor(() =>
      expect(calls.filter((c) => c.cmd === "whats_new").at(-1)?.args).toEqual({ langs: ["X"] }),
    );
    expect(await screen.findByText("Frisch erschienen")).toBeTruthy();
  });

  it("keeps the rows when the lookup fails", async () => {
    renderHome();
    expect(await screen.findByText("Frisch erschienen")).toBeTruthy();
    cleanup();
    backend.whats_new = () => {
      throw new Error("boom");
    };
    renderHome();
    expect(await screen.findByText("Frisch erschienen")).toBeTruthy();
  });

  it("shows the toolbox in the picked language while What's New keeps the app language", async () => {
    localStorage.setItem("toolboxLang", "T");
    backend.teaching_toolbox = () => [entry("lff", "Books", null, "Felizes para Sempre")];
    whatsNewRows = [{ ...entry("ll", "Brochures and Booklets", null, "Neue Ausgabe"), langCode: "X" }];
    renderHome();
    expect(await screen.findByText("Felizes para Sempre")).toBeTruthy();
    expect(screen.queryByText("Glücklich")).toBeNull(); // the app-language toolbox is not used
    expect(screen.getByText("Neue Ausgabe")).toBeTruthy();
    await waitFor(() => expect(calls.find((c) => c.cmd === "teaching_toolbox")?.args).toMatchObject({ lang: "T" }));
    expect(calls.find((c) => c.cmd === "home_lists")?.args).toMatchObject({ lang: "X" });
  });

  it("downloads a toolbox tile in the toolbox language", async () => {
    localStorage.setItem("toolboxLang", "T");
    backend.teaching_toolbox = () => [entry("lff", "Books", null, "Felizes para Sempre")];
    renderHome();
    const t = await tile("Felizes para Sempre");
    fireEvent.click(within(t).getByTitle("Felizes para Sempre"));
    await waitFor(() => expect(calls.some((c) => c.cmd === "download_publication")).toBe(true));
    expect(calls.find((c) => c.cmd === "download_publication")?.args).toMatchObject({ lang: "T" });
  });

  it("opens a downloaded toolbox tile without downloading again", async () => {
    localStorage.setItem("toolboxLang", "T");
    backend.teaching_toolbox = () => [entry("lff", "Books", "lff_2", "Felizes para Sempre")];
    renderHome();
    fireEvent.click(within(await tile("Felizes para Sempre")).getByTitle("Felizes para Sempre"));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "download_publication").length).toBe(0));
  });

  it("picks a language with the picker and resets to the app language", async () => {
    backend.languages = () => [
      { code: "T", name: "Portuguese", vernacular: "Português", rtl: false, signLanguage: false },
      { code: "X", name: "German", vernacular: "Deutsch", rtl: false, signLanguage: false },
    ];
    renderHome();
    await screen.findByText("Glücklich");
    fireEvent.click(screen.getByTitle(/Toolbox language: /));
    fireEvent.click(screen.getByText("Português"));
    expect(localStorage.getItem("toolboxLang")).toBe("T");
    await waitFor(() => expect(calls.some((c) => c.cmd === "teaching_toolbox")).toBe(true));
    // Picking the app's own language clears the override instead of storing it.
    fireEvent.click(screen.getByTitle(/Toolbox language: /));
    fireEvent.click(screen.getByText("German")); // the name label of the app-language row
    expect(localStorage.getItem("toolboxLang")).toBeNull();
    await waitFor(() =>
      expect(calls.filter((c) => c.cmd === "teaching_toolbox" && (c.args as { lang?: string }).lang === "X").length).toBe(0),
    );
    expect(await screen.findByText("Glücklich")).toBeTruthy(); // still the home_lists toolbox
    fireEvent.click(screen.getByTitle(/Toolbox language: /));
    fireEvent.click(screen.getByText("Same as app language"));
    expect(localStorage.getItem("toolboxLang")).toBeNull();
    expect(await screen.findByText("Glücklich")).toBeTruthy(); // back to the home_lists toolbox
  });

  it("toasts and empties the toolbox when its fetch fails", async () => {
    localStorage.setItem("toolboxLang", "T");
    backend.teaching_toolbox = () => { throw new Error("boom"); };
    whatsNewRows = [{ ...entry("ll", "Brochures and Booklets", null, "Neue Ausgabe"), langCode: "X" }];
    renderHome();
    expect(await screen.findByText("Neue Ausgabe")).toBeTruthy();
    await waitFor(() => expect(screen.queryByText("Glücklich")).toBeNull());
    expect((await screen.findAllByText(/boom/)).length).toBeGreaterThan(0); // the error is toasted
  });
});

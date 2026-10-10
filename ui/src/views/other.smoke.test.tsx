// @vitest-environment jsdom
// Renders the search, meetings, updates and publication views in StrictMode with a mocked backend.
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
import { MeetingsView } from "@/views/meetings";
import { OnlineView } from "@/views/online";
import { PublicationView } from "@/views/publication";
import { UpdatesTab } from "@/views/updates";
import { SettingsView } from "@/views/settings";
import { PersonalView } from "@/views/personal";
import { GoTo } from "@/components/go-to";

beforeEach(() => {
  calls.length = 0;
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
const openMenu = (t: HTMLElement) => fireEvent.click(within(t).getByLabelText("More"));

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
});

describe("Updates", () => {
  it("lists an update with a menu that can remove the installed version", async () => {
    backend.check_updates = () => [
      { dir: "lff_2", langCode: "X", installedSize: 1, entry: entry("lff", "Books", "lff_2", "Glücklich") },
    ];
    renderIn(<UpdatesTab />);
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
    fireEvent.click(await screen.findByRole("button", { name: "Full width" }));
    expect(localStorage.getItem("layout")).toBe("wide");
    expect(document.documentElement.style.getPropertyValue("--page-max")).toBe("100000px");
    expect(screen.getByRole("button", { name: "Full width" }).getAttribute("aria-pressed")).toBe("true");
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
    fireEvent.change(screen.getByLabelText("Sort notes"), { target: { value: "oldest" } });
    expect(titles()).toEqual(["Dusk visits", "Patient listening"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "patient" } });
    expect(titles()).toEqual(["Patient listening"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Yellow" }));
    expect(titles()).toEqual(["Dusk visits"]);
    fireEvent.change(screen.getByLabelText("Search notes"), { target: { value: "zzz" } });
    expect(screen.getByText("No notes match.")).toBeTruthy();
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
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    fireEvent.keyDown(window, { key: "g", ctrlKey: true });
    const again = await screen.findByLabelText("Reference, for example Heb 10:24");
    await waitFor(() => expect((screen.getByRole("button", { name: "Go" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.change(again, { target: { value: "Heb 10:24" } });
    fireEvent.submit(again.closest("form")!);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

// @vitest-environment jsdom
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within, cleanup } from "@testing-library/react";
import type { PubCard, SearchResponse, SearchStatus, Target } from "@/lib/api";
import { card } from "@/lib/fixtures";

const calls: { cmd: string; args: any }[] = [];
let publications: PubCard[] = [];
let response: SearchResponse;
let status: SearchStatus;
let runSearch: ((request: any) => unknown) | null = null;
let rebuilds = 0;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: any) => {
    calls.push({ cmd, args });
    if (cmd === "list_publications") return publications;
    if (cmd === "languages") return [];
    if (cmd === "favorites") return [];
    if (cmd === "catalog_cached") return true;
    if (cmd === "check_updates") return null;
    if (cmd === "running_jobs") return 0;
    if (cmd === "media_downloads") return [];
    if (cmd === "auto_settings") return { rolling: false, keep: 0, syncFolder: null };
    if (cmd === "sync_status") return { state: "idle", file: null };
    if (cmd === "search_status") return status;
    if (cmd === "search_publications") return runSearch ? runSearch(args.request) : response;
    if (cmd === "rebuild_search") { rebuilds++; return; }
    throw new Error(`unmocked command ${cmd}`);
  }),
  convertFileSrc: (path: string) => path,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined), emitTo: vi.fn() }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ label: "main", onCloseRequested: async () => () => undefined }),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.0.0" }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { AppBar, AppProvider, useApp } from "@/app";
import { SearchView } from "@/views/search";

const target: Target = { publication: "w26_2", kind: { documentParagraph: { document: 4, paragraph: 12 } } };

beforeEach(() => {
  calls.length = 0;
  runSearch = null;
  rebuilds = 0;
  localStorage.clear();
  localStorage.setItem("lang", "X");
  publications = [
    card({ dir: "w26_2", title: "The Watchtower", symbol: "w26" }),
    card({ dir: "nwt_E", title: "Study Bible", symbol: "nwt", isBible: true }),
  ];
  status = { state: "ready", total: 2, indexed: 2, failed: 0, unanchored: 0 };
  response = {
    hits: [{ title: "Lesson", publicationTitle: "The Watchtower", snippet: "Read <script>alert(1)</script> safely", target }],
    status,
  };
});
afterEach(cleanup);

function Where() {
  const { view } = useApp();
  return <div data-testid="where">{JSON.stringify(view)}</div>;
}

const renderSearch = () =>
  render(
    <StrictMode>
      <AppProvider>
        <SearchView />
        <Where />
      </AppProvider>
    </StrictMode>,
  );

describe("SearchView", () => {
  it("opens library search from the existing app bar", () => {
    render(<AppProvider><AppBar title="Home" /><Where /></AppProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Search library" }));
    expect(JSON.parse(screen.getByTestId("where").textContent!)).toEqual({ name: "search" });
  });

  it("reports incomplete coverage and lets the user request a rebuild", async () => {
    status = { state: "incomplete", total: 2, indexed: 1, failed: 1, unanchored: 0 };
    renderSearch();
    expect(await screen.findByText(/Search index is incomplete/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry indexing" }));
    await waitFor(() => expect(rebuilds).toBe(1));
  });

  it("shows indexing and unavailable coverage", async () => {
    status = { state: "indexing", total: 2, indexed: 1, failed: 0, unanchored: 0 };
    const view = renderSearch();
    expect(await screen.findByText(/Indexing downloaded content/)).toBeTruthy();
    view.unmount();
    status = { state: "unavailable", total: 2, indexed: 0, failed: 0, unanchored: 0 };
    renderSearch();
    expect(await screen.findByText(/Search is unavailable/)).toBeTruthy();
  });

  it("shows loading, empty results and backend errors", async () => {
    let resolve!: (value: SearchResponse) => void;
    runSearch = () => new Promise<SearchResponse>((done) => { resolve = done; });
    renderSearch();
    const query = screen.getByRole("searchbox", { name: "Search library" });
    fireEvent.change(query, { target: { value: "missing" } });
    expect(await screen.findByText("Searching…")).toBeTruthy();
    await waitFor(() => expect(resolve).toBeDefined());
    resolve({ hits: [], status });
    expect(await screen.findByText("Nothing found.")).toBeTruthy();
    runSearch = () => { throw new Error("database failed"); };
    fireEvent.change(query, { target: { value: "error" } });
    expect(await screen.findByText(/database failed/)).toBeTruthy();
    fireEvent.change(query, { target: { value: '"unfinished' } });
    expect(await screen.findByText(/Unmatched quotation mark/)).toBeTruthy();
  });

  it("ignores older responses after a later query", async () => {
    const pending: Record<string, (value: SearchResponse) => void> = {};
    runSearch = (request) => new Promise<SearchResponse>((done) => { pending[request.query] = done; });
    renderSearch();
    const query = screen.getByRole("searchbox", { name: "Search library" });
    fireEvent.change(query, { target: { value: "first" } });
    await waitFor(() => expect(pending.first).toBeDefined());
    fireEvent.change(query, { target: { value: "second" } });
    await waitFor(() => expect(pending.second).toBeDefined());
    pending.second({ hits: [{ ...response.hits[0], title: "New result" }], status });
    expect(await screen.findByText("New result")).toBeTruthy();
    pending.first({ hits: [{ ...response.hits[0], title: "Old result" }], status });
    await new Promise((done) => setTimeout(done, 20));
    expect(screen.queryByText("Old result")).toBeNull();
    expect(screen.getByText("New result")).toBeTruthy();
  });

  it("requests successive result pages and returns to the previous page", async () => {
    runSearch = (request) => ({
      status,
      hits: request.offset === 0
        ? Array.from({ length: 50 }, (_, index) => ({ ...response.hits[0], title: `First ${index}` }))
        : [{ ...response.hits[0], title: "Second page" }],
    });
    renderSearch();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search library" }), { target: { value: "page" } });
    expect(await screen.findByText("First 0")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByText("Second page")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Previous page" }));
    expect(await screen.findByText("First 0")).toBeTruthy();
  });

  it("searches all content, exposes Bible and publication scopes, and renders snippets as text", async () => {
    renderSearch();

    const query = await screen.findByRole("searchbox", { name: "Search library" });
    const scope = screen.getByRole("combobox", { name: "Search scope" });
    expect(within(scope).getByRole("option", { name: "All downloaded content" })).toBeTruthy();
    expect(within(scope).getByRole("option", { name: "Downloaded Bibles" })).toBeTruthy();
    expect(within(scope).getByRole("option", { name: "The Watchtower" })).toBeTruthy();

    fireEvent.change(query, { target: { value: "lesson" } });
    await waitFor(() =>
      expect(calls.some((call) => call.cmd === "search_publications")).toBe(true),
    );
    expect(calls.find((call) => call.cmd === "search_publications")?.args).toEqual({
      request: { query: "lesson", scope: { kind: "all" }, limit: 50, offset: 0 },
    });
    expect(screen.getByText("Read <script>alert(1)</script> safely")).toBeTruthy();
    expect(document.querySelector("script")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Lesson/ }));
    expect(JSON.parse(screen.getByTestId("where").textContent!)).toEqual({ name: "reader", target, fromSearch: true });

    fireEvent.change(scope, { target: { value: "bibles" } });
    await waitFor(() =>
      expect(calls.some((call) => call.args?.request?.scope?.kind === "bibles")).toBe(true),
    );
    fireEvent.change(scope, { target: { value: "publication:w26_2" } });
    await waitFor(() =>
      expect(calls.some((call) => call.args?.request?.scope?.dirName === "w26_2")).toBe(true),
    );
  });
});

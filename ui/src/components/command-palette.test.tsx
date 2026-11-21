// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { PubCard, SearchResponse, Target } from "@/lib/api";

const { push, searchPublications } = vi.hoisted(() => ({ push: vi.fn(), searchPublications: vi.fn() }));

vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api")>("@/lib/api");
  return { ...actual, api: { ...actual.api, searchPublications } };
});
const publications: PubCard[] = [
  { dir: "w26_1", title: "The Watchtower", shortTitle: "Watchtower", symbol: "w", keySymbol: "w", year: 2026, issueTag: "1", mepsLanguage: 0, langCode: "X", publicationType: null, isBible: false, cover: null },
  { dir: "lff", title: "Enjoy Life Forever!", shortTitle: "Enjoy Life", symbol: "lff", keySymbol: "lff", year: 2026, issueTag: "", mepsLanguage: 0, langCode: "X", publicationType: null, isBible: false, cover: null },
];

vi.mock("@/app", () => ({
  useApp: () => ({ publications, push, openTarget: vi.fn(), toast: vi.fn(), lang: "X", bibles: {} }),
}));

import { CommandPalette } from "./command-palette";

afterEach(() => {
  cleanup();
  push.mockReset();
  searchPublications.mockReset();
});

describe("CommandPalette", () => {
  it("opens with Ctrl+K and Meta+K, but ignores Alt combinations", () => {
    render(<CommandPalette />);

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", altKey: true, ctrlKey: true });
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("closes with Escape", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders action and publication groups and filters publication titles locally", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });

    expect(screen.getByText("Actions")).toBeTruthy();
    expect(screen.getByText("Publications")).toBeTruthy();
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.getByText("Enjoy Life Forever!")).toBeTruthy();

    fireEvent.change(screen.getByRole("combobox"), { target: { value: "watchtower" } });
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.queryByText("Enjoy Life Forever!")).toBeNull();
  });

  it("clears the query and results when closed and reopened", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = screen.getByRole("combobox");

    fireEvent.change(input, { target: { value: "watchtower" } });
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.queryByText("Enjoy Life Forever!")).toBeNull();

    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect((screen.getByRole("combobox") as HTMLInputElement).value).toBe("");
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    expect(screen.getByText("Enjoy Life Forever!")).toBeTruthy();
  });

  it("opens a publication when its command is selected", () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.click(screen.getByText("The Watchtower"));
    expect(push).toHaveBeenCalledWith({ name: "publication", dir: "w26_1" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("loads debounced full-text previews and opens a result", async () => {
    const target: Target = { publication: "w26_1", kind: { documentParagraph: { document: 1, paragraph: 2 } } };
    const response: SearchResponse = { hits: [{ title: "Search hit", publicationTitle: "The Watchtower", snippet: "A matching excerpt", target }], status: { state: "ready", total: 1, indexed: 1, failed: 0, unanchored: 0 } };
    searchPublications.mockResolvedValue(response);
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "matching" } });
    expect(screen.queryByText("Search hit")).toBeNull();
    expect(await screen.findByText("Search hit", {}, { timeout: 1000 })).toBeTruthy();
    expect(searchPublications).toHaveBeenCalledWith({ query: "matching", scope: { kind: "all" }, limit: 8, offset: 0 });
    fireEvent.click(screen.getByText("Search hit"));
    expect(push).toHaveBeenCalledWith({ name: "reader", target, fromSearch: true });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps working when typing on after full-text results appeared", async () => {
    const target: Target = { publication: "bible", kind: { documentParagraph: { document: 1, paragraph: 1 } } };
    searchPublications.mockResolvedValue({ hits: [{ title: "John 1", publicationTitle: "Bible", snippet: "In the beginning", target }], status: { state: "ready", total: 1, indexed: 1, failed: 0, unanchored: 0 } });
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.queryByText("Full-text results")).toBeNull();
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "Jo 1" } });
    expect(await screen.findByText("John 1", {}, { timeout: 1000 })).toBeTruthy();
    fireEvent.change(input, { target: { value: "Jo 1:" } });
    expect(await screen.findByText("John 1", {}, { timeout: 1000 })).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("opens Search with the palette query", async () => {
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "phrase" } });
    await waitFor(() => expect(screen.getByText("Search all text")).toBeTruthy());
    fireEvent.click(screen.getByText("Search all text"));
    expect(push).toHaveBeenCalledWith({ name: "search", query: "phrase" });
  });

  it("keeps local commands and publications usable when full-text search fails", async () => {
    searchPublications.mockRejectedValue(new Error("search unavailable"));
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "watchtower" } });

    await waitFor(() => expect(searchPublications).toHaveBeenCalled());
    expect(screen.getByText("The Watchtower")).toBeTruthy();
    fireEvent.click(screen.getByText("The Watchtower"));
    expect(push).toHaveBeenCalledWith({ name: "publication", dir: "w26_1" });
  });

  it("ignores stale full-text responses after a later query", async () => {
    const pending: Record<string, (response: SearchResponse) => void> = {};
    searchPublications.mockImplementation(({ query }: { query: string }) => new Promise<SearchResponse>((resolve) => {
      pending[query] = resolve;
    }));
    render(<CommandPalette />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "first" } });
    await waitFor(() => expect(pending.first).toBeDefined());
    fireEvent.change(input, { target: { value: "second" } });
    await waitFor(() => expect(pending.second).toBeDefined());
    const result = (title: string): SearchResponse => ({
      hits: [{ title, publicationTitle: "The Watchtower", snippet: "excerpt", target: { publication: "w26_1", kind: { documentParagraph: { document: 1, paragraph: 1 } } } }],
      status: { state: "ready", total: 1, indexed: 1, failed: 0, unanchored: 0 },
    });
    pending.second(result("second current"));
    expect(await screen.findByText("second current")).toBeTruthy();
    pending.first(result("second stale"));
    await waitFor(() => expect(screen.queryByText("second stale")).toBeNull());
    expect(screen.getByText("second current")).toBeTruthy();
  });
});

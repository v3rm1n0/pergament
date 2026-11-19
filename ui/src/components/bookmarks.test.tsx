// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, chapterTarget, type Bookmark } from "@/lib/api";
import { BookmarksButton } from "./bookmarks";

const app = vi.hoisted(() => ({ push: vi.fn(), toast: vi.fn(), notesChanged: vi.fn() }));
vi.mock("@/app", () => ({
  useApp: () => app,
  BarButton: ({ label, children, onClick }: { label: string; children: React.ReactNode; onClick: () => void }) =>
    <button aria-label={label} onClick={onClick}>{children}</button>,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const target = chapterTarget("bible", 19, 23);
const position = { blockType: 2, blockIdentifier: 4, snippet: "Reading position" };
const bookmark: Bookmark = {
  slot: 0, title: "Psalm 23", ...position,
  location: { keySymbol: "nwtsty", mepsLanguage: 2, issueTag: 0, documentId: null, book: 19, chapter: 23, title: "Psalm 23" },
};

beforeEach(() => {
  vi.spyOn(api, "publicationBookmarks").mockResolvedValue([]);
  vi.spyOn(api, "saveBookmark").mockResolvedValue();
  vi.spyOn(api, "deleteBookmark").mockResolvedValue();
  vi.spyOn(api, "openLocation").mockResolvedValue(target);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.clearAllMocks(); });

describe("Publication bookmarks", () => {
  it("loads only when opened and saves the captured position to one of ten slots", async () => {
    const current = vi.fn(() => ({ target, position }));
    render(<BookmarksButton dir="bible" title="Study Bible" current={current} />);
    expect(api.publicationBookmarks).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    const slots = await screen.findAllByRole("button", { name: /^Save bookmark \d+$/ });
    expect(slots).toHaveLength(10);
    expect(current).toHaveBeenCalledTimes(1);
    expect(api.publicationBookmarks).toHaveBeenCalledWith("bible");
    fireEvent.click(slots[9]);
    await waitFor(() => expect(api.saveBookmark).toHaveBeenCalledWith(target, { slot: 9, ...position }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(app.notesChanged).toHaveBeenCalledOnce();
  });

  it("opens a saved bookmark with its paragraph or verse anchor", async () => {
    vi.mocked(api.publicationBookmarks).mockResolvedValue([bookmark]);
    render(<BookmarksButton dir="bible" title="Study Bible" />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Open bookmark 1: Psalm 23" }));
    await waitFor(() => expect(app.push).toHaveBeenCalledWith({
      name: "reader", target, bookmark: { blockType: 2, blockIdentifier: 4 },
    }));
    expect(api.saveBookmark).not.toHaveBeenCalled();
  });

  it("keeps empty slots disabled outside the reader", async () => {
    render(<BookmarksButton dir="bible" title="Study Bible" />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    const slot = await screen.findByRole("button", { name: "Save bookmark 1" });
    expect((slot as HTMLButtonElement).disabled).toBe(true);
    expect(slot.textContent).toBe("");
  });

  it("replaces an occupied slot only through its explicit menu action", async () => {
    vi.mocked(api.publicationBookmarks).mockResolvedValue([bookmark]);
    render(<BookmarksButton dir="bible" title="Study Bible" current={() => ({ target, position })} />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.pointerDown(await screen.findByRole("button", { name: "Bookmark 1 options" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Replace with current location" }));
    await waitFor(() => expect(api.saveBookmark).toHaveBeenCalledWith(target, { slot: 0, ...position }));
    expect(api.openLocation).not.toHaveBeenCalled();
    expect(app.notesChanged).toHaveBeenCalledOnce();
  });

  it("deletes a bookmark and leaves its slot available to save again", async () => {
    vi.mocked(api.publicationBookmarks).mockResolvedValue([bookmark]);
    render(<BookmarksButton dir="bible" title="Study Bible" current={() => ({ target, position })} />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.pointerDown(await screen.findByRole("button", { name: "Bookmark 1 options" }), { button: 0, ctrlKey: false });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
    await waitFor(() => expect(api.deleteBookmark).toHaveBeenCalledWith("bible", 0));
    expect(await screen.findByRole("button", { name: "Save bookmark 1" })).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(app.notesChanged).toHaveBeenCalledOnce();
  });

  it("allows retrying a failed bookmark load", async () => {
    vi.mocked(api.publicationBookmarks).mockRejectedValueOnce(new Error("Cannot read bookmarks"));
    render(<BookmarksButton dir="bible" title="Study Bible" />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("button", { name: "Save bookmark 1" })).toBeTruthy();
    expect(app.toast).toHaveBeenCalledWith("Error: Cannot read bookmarks");
  });

  it("shows a failed save and keeps the dialog open", async () => {
    vi.mocked(api.saveBookmark).mockRejectedValue(new Error("Disk full"));
    render(<BookmarksButton dir="bible" title="Study Bible" current={() => ({ target, position })} />);
    fireEvent.click(screen.getByRole("button", { name: "Bookmarks" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save bookmark 1" }));
    await waitFor(() => expect(app.toast).toHaveBeenCalledWith("Error: Disk full"));
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(app.notesChanged).not.toHaveBeenCalled();
  });
});

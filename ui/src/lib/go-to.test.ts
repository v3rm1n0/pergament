import { describe, expect, it, vi } from "vitest";
import { openBibleReference } from "./go-to";
import { namedBooks, type Book } from "./bible";
import { api, chapterTarget, type Target } from "./api";

const books = namedBooks([], [{ number: 43, title: "John", chapters: 21 }], { 43: "Joh" });

const options = (): {
  bible: { dir: string; books: Book[] } | null;
  toast: (text: string) => void;
  open: (target: Target) => void;
} => ({
  bible: { dir: "nwt_X", books },
  toast: vi.fn(),
  open: vi.fn(),
});

vi.spyOn(api, "lastVerse").mockImplementation(async (_dir, _book, _chapter) => 21);

describe("openBibleReference", () => {
  it("opens the first verse of a valid chapter and verse", async () => {
    const opts = options();

    expect(await openBibleReference("Joh 3:16", opts)).toBe(true);
    expect(opts.open).toHaveBeenCalledWith(chapterTarget("nwt_X", 43, 3, 16));
  });

  it("rejects a verse beyond the chapter bounds", async () => {
    const opts = options();

    expect(await openBibleReference("Joh 3:22", opts)).toBe(false);
    expect(opts.open).not.toHaveBeenCalled();
  });

  it("rejects when no Bible is available", async () => {
    const opts = options();
    opts.bible = null;

    expect(await openBibleReference("Joh 3:16", opts)).toBe(false);
    expect(opts.open).not.toHaveBeenCalled();
  });

  it("rejects malformed text", async () => {
    const opts = options();

    expect(await openBibleReference("not a reference", opts)).toBe(false);
    expect(opts.open).not.toHaveBeenCalled();
  });
});

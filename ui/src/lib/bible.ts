import type { TocNode } from "./api";

/** Tile shade per group of Bible books (genre), darkest = 0. */
export type Shade = 0 | 1 | 2;

const GROUPS: [number, number, Shade][] = [
  [1, 5, 0], // Pentateuch
  [6, 17, 2], // history
  [18, 22, 1], // poetry and wisdom
  [23, 39, 0], // prophets
  [40, 43, 0], // gospels
  [44, 44, 2], // Acts
  [45, 65, 1], // letters
  [66, 66, 0], // Revelation
];

export function bookShade(book: number): Shade {
  return GROUPS.find(([a, b]) => book >= a && book <= b)?.[2] ?? 1;
}

/** "1. Mose (Genesis)" -> "1. Mose" for compact tiles. */
export function shortBookName(title: string): string {
  return title.replace(/\s*\([^)]*\)\s*$/, "").trim();
}

/** Does this subtree contain Bible books? */
export function hasBooks(node: TocNode): boolean {
  return node.bible_book != null || node.children.some(hasBooks);
}

/** Index of the tab that holds the books (the default Bible tab). */
export function booksTabIndex(toc: TocNode[]): number {
  const i = toc.findIndex(hasBooks);
  return i < 0 ? 0 : i;
}


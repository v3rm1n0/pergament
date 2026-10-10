import type { TocNode } from "./api";

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


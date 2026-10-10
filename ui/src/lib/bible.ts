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


/**
 * The books of a Bible with the names the table of contents gives them ("Hebräer"), since the book list itself has
 * the long ones ("Der Brief an die Hebräer"). Books the contents do not name keep their long name.
 */
export function namedBooks(
  toc: TocNode[],
  books: { number: number; title: string; chapters: number }[],
): { number: number; title: string; chapters: number }[] {
  const names = new Map<number, string>();
  const walk = (nodes: TocNode[]) =>
    nodes.forEach((n) => {
      if (n.bible_book != null && !names.has(n.bible_book)) names.set(n.bible_book, n.title);
      walk(n.children);
    });
  walk(toc);
  return books.map((b) => ({ number: b.number, chapters: b.chapters, title: names.get(b.number) ?? b.title }));
}

/** A place in a Bible: chapter, and the verse if one was given. */
export interface Reference {
  book: number;
  chapter: number;
  verse: number | null;
}

const plain = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase()
    .replace(/\./g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** The names a book answers to: the shown name and the one in brackets, such as "1. Mose" and "Genesis". */
function bookNames(title: string): string[] {
  const inner = title.match(/\(([^)]*)\)\s*$/)?.[1];
  return [shortBookName(title), ...(inner ? [inner] : [])].map(plain).filter(Boolean);
}

/**
 * Reads a reference such as "Heb 10:24", "1 Joh 3:16", "1. Mose 2" or "Psalm 23" against the books of a Bible. A name
 * only has to start a book's name ("Heb" for Hebrews), and a book number may be written with or without a dot. Returns
 * null when no book fits or the chapter does not exist.
 */
export function parseReference(input: string, books: { number: number; title: string; chapters: number }[]): Reference | null {
  const m = input.trim().match(/^(\d)?[.\s]*(\p{L}[\p{L}.\s]*?)\s*(?:(\d+)(?:\s*[:,.]\s*(\d+))?)?$/u);
  if (!m) return null;
  const query = plain(`${m[1] ?? ""} ${m[2]}`);
  const named = (names: string[], test: (n: string) => boolean) => names.some(test);
  const book =
    books.find((b) => named(bookNames(b.title), (n) => n === query)) ??
    books
      .filter((b) => named(bookNames(b.title), (n) => n.startsWith(query)))
      .sort((a, b) => a.title.length - b.title.length || a.number - b.number)[0];
  if (!book) return null;
  // A single number after a book of one chapter ("Jude 5") is the verse.
  if (book.chapters === 1 && m[3] && !m[4]) return { book: book.number, chapter: 1, verse: Number(m[3]) };
  const chapter = m[3] ? Number(m[3]) : 1;
  if (chapter < 1 || chapter > book.chapters) return null;
  return { book: book.number, chapter, verse: m[4] ? Number(m[4]) : null };
}

import type { Note } from "./api";

export type NoteSort = "newest" | "oldest" | "publication";

export interface NoteFilter {
  query: string;
  /** Highlight color index, or null for any. */
  color: number | null;
  sort: NoteSort;
}

const haystack = (n: Note) =>
  [n.title, n.content, n.location?.title ?? "", ...n.tags].join("\n").toLowerCase();

/** Notes matching a text (every word must occur) and a highlight color, in the chosen order. */
export function filterNotes(notes: Note[], { query, color, sort }: NoteFilter): Note[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const shown = notes.filter((n) => (color === null || n.color === color) && words.every((w) => haystack(n).includes(w)));
  const byDate = (a: Note, b: Note) => a.lastModified.localeCompare(b.lastModified);
  return [...shown].sort((a, b) =>
    sort === "oldest"
      ? byDate(a, b)
      : sort === "publication"
        ? (a.location?.title ?? "￿").localeCompare(b.location?.title ?? "￿") || byDate(b, a)
        : byDate(b, a),
  );
}

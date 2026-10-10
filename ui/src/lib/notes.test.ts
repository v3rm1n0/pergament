import { describe, expect, it } from "vitest";
import type { Note } from "./api";
import { filterNotes } from "./notes";

const note = (guid: string, over: Partial<Note> = {}): Note => ({
  guid,
  title: "",
  content: "",
  blockType: 0,
  blockIdentifier: null,
  markGuid: null,
  color: null,
  tags: [],
  lastModified: "2026-10-01T00:00:00Z",
  location: null,
  ...over,
});

const notes = [
  note("a", { title: "Dusk visits", content: "Bring tea", tags: ["Family"], color: 1, lastModified: "2026-10-03T00:00:00Z" }),
  note("b", { title: "Patient listening", content: "Count to five", tags: ["Family", "Practice"], color: 3, lastModified: "2026-10-05T00:00:00Z", location: { title: "Lanterns 10" } as Note["location"] }),
  note("c", { title: "Rehearsal", content: "Record the talk", lastModified: "2026-10-01T00:00:00Z", location: { title: "Gathering Workbook" } as Note["location"] }),
];
const ids = (list: Note[]) => list.map((n) => n.guid);
const any = { query: "", color: null, sort: "newest" as const };

describe("filterNotes", () => {
  it("orders by date, newest first by default", () => {
    expect(ids(filterNotes(notes, any))).toEqual(["b", "a", "c"]);
    expect(ids(filterNotes(notes, { ...any, sort: "oldest" }))).toEqual(["c", "a", "b"]);
  });

  it("orders by publication, notes without one last", () => {
    expect(ids(filterNotes(notes, { ...any, sort: "publication" }))).toEqual(["c", "b", "a"]);
  });

  it("matches every word in the title, text, tags or publication, ignoring case", () => {
    expect(ids(filterNotes(notes, { ...any, query: "family five" }))).toEqual(["b"]);
    expect(ids(filterNotes(notes, { ...any, query: "LANTERNS" }))).toEqual(["b"]);
    expect(ids(filterNotes(notes, { ...any, query: "nothing here" }))).toEqual([]);
  });

  it("filters by highlight color", () => {
    expect(ids(filterNotes(notes, { ...any, color: 1 }))).toEqual(["a"]);
    expect(ids(filterNotes(notes, { ...any, color: 6 }))).toEqual([]);
  });
});

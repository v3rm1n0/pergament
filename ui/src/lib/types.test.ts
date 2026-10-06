import { describe, expect, it } from "vitest";
import type { CatalogEntry, PubCard } from "@/lib/api";
import { groupByCategory, importedOnly, mergeCategories, sectionsFor } from "@/lib/types";

const pub = (dir: string, publicationType: string | null, year = 2026): PubCard => ({
  dir,
  title: dir,
  shortTitle: null,
  symbol: dir,
  year,
  issueTag: "0",
  mepsLanguage: 2,
  langCode: "X",
  publicationType,
  isBible: publicationType === "Bible",
  cover: null,
});

describe("publication types", () => {
  it("groups by manifest type in display order", () => {
    const groups = groupByCategory([pub("a", "Curriculum"), pub("b", "Brochure"), pub("c", "Booklet"), pub("d", null)]);
    expect(groups.map((g) => [g.name, g.pubs.length])).toEqual([
      ["Brochures and Booklets", 2],
      ["Curriculum", 1],
      ["Other", 1],
    ]);
  });

  it("adds categories only imported publications have", () => {
    const merged = mergeCategories([{ id: 4, name: "Brochures and Booklets", count: 9 }], [
      pub("a", "Brochure"),
      pub("b", "Outline"),
    ]);
    expect(merged.map((c) => c.name)).toEqual(["Brochures and Booklets", "Outlines"]);
  });

  it("lists imported publications the catalog lacks", () => {
    const listed = { local: "a" } as CatalogEntry;
    const extra = importedOnly([listed], [pub("a", "Brochure"), pub("b", "Brochure"), pub("c", "Book")], 4);
    expect(extra.map((e) => e.local)).toEqual(["b"]);
  });
});

describe("category sections", () => {
  const entry = (symbol: string, year: number, attributes: string[] = []): CatalogEntry =>
    ({ local: null, item: { symbol, year, issue_tag: 0, attributes } }) as unknown as CatalogEntry;
  const shape = (s: ReturnType<typeof sectionsFor>) =>
    s.map((x) => [x.title, x.parts.map((p) => [p.title, p.entries.map((e) => e.item.symbol)])]);

  it("puts newer publications first without a heading", () => {
    const s = sectionsFor(2, [entry("old", 1980, ["Archive"]), entry("yb", 2020, ["Yearbook"]), entry("new", 2024)]);
    expect(s.map((x) => x.title)).toEqual([null, "Yearbooks", "Older Publications"]);
  });

  it("splits tracts into convention, older and Kingdom News", () => {
    const s = sectionsFor(10, [
      entry("kn", 2000, ["Kingdom News"]),
      entry("T", 2025),
      entry("old", 1990, ["Archive"]),
      entry("CO-inv26", 2026, ["Convention", "Invitation"]),
    ]);
    expect(s.map((x) => x.title)).toEqual([null, "Convention", "Older Publications", "Kingdom News"]);
  });

  it("groups Watchtower by year, then edition", () => {
    const s = sectionsFor(14, [
      entry("w", 2026, ["Study"]),
      entry("wp", 2026, ["Public"]),
      entry("w", 2005),
      entry("w", 2025, ["Study"]),
    ]);
    expect(shape(s)).toEqual([
      ["2026", [["Public Edition", ["wp"]], ["Study Edition", ["w"]]]],
      ["2025", [["Study Edition", ["w"]]]],
      ["2005", [[null, ["w"]]]],
    ]);
  });

  it("splits programs", () => {
    const s = sectionsFor(31, [entry("CO-pgm26", 2026, ["Convention"]), entry("CA-copgm25", 2025, ["Circuit Assembly"])]);
    expect(s.map((x) => x.title)).toEqual(["Circuit Assembly", "Convention Program"]);
  });
});

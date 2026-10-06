import { describe, expect, it } from "vitest";
import type { CatalogEntry, PubCard } from "@/lib/api";
import { groupByCategory, importedOnly, mergeCategories } from "@/lib/types";

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

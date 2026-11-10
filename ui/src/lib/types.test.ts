import { describe, expect, it } from "vitest";
import type { CatalogEntry, PubCard } from "@/lib/api";
import { setUiLang } from "@/lib/i18n";
import { categoryOf, groupByCategory, importedOnly, mergeCategories, sectionsFor } from "@/lib/types";

const pub = (dir: string, publicationType: string | null, year = 2026): PubCard => ({
  dir,
  title: dir,
  shortTitle: null,
  symbol: dir,
  keySymbol: dir,
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

  it("preserves the database identity of imported-only dated publications", () => {
    const publication = {
      ...pub("w26_0_20260800", "Watchtower"),
      symbol: "w26",
      keySymbol: "w",
      issueTag: "20260800",
      mepsLanguage: 0,
      langCode: "E",
    };
    const [extra] = importedOnly([], [publication], categoryOf(publication).id);
    expect(extra.item.key_symbol).toBe("w");
    expect(extra.item.symbol).toBe("w26");
    expect(extra.item.meps_language).toBe(0);
    expect(extra.item.issue_tag).toBe(20260800);
    expect(extra.local).toBe(publication.dir);
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

  it("sorts Manual/Guidelines into Guidelines, not Other", () => {
    expect(categoryOf({ publicationType: "Manual/Guidelines" })).toEqual({ id: 17, name: "Guidelines" });
    expect(categoryOf({ publicationType: "Guidelines" }).id).toBe(17);
  });

  it("sorts talks into Outlines, not Other", () => {
    expect(categoryOf({ publicationType: "Talk" })).toEqual({ id: -3, name: "Outlines" });
  });

  it("splits Guidelines by first attribute without knowing the names", () => {
    const s = sectionsFor(17, [
      entry("a", 2020, ["Meetings"]),
      entry("b", 2021, ["Zeta", "Meetings"]),
      entry("c", 2022),
      entry("d", 2019, ["Alpha"]),
    ]);
    expect(shape(s)).toEqual([
      [null, [[null, ["c"]]]],
      ["Alpha", [[null, ["d"]]]],
      ["Meetings", [[null, ["a"]]]],
      ["Zeta", [[null, ["b"]]]],
    ]);
  });

  it("translates the Guidelines headings it knows", () => {
    setUiLang("de");
    try {
      const s = sectionsFor(17, [entry("a", 2020, ["Meetings"]), entry("b", 2021, ["Alpha"])]);
      expect(s.map((x) => x.title)).toEqual(["Alpha", "Zusammenkünfte"]);
    } finally {
      setUiLang("en");
    }
  });

  it("shows one untitled Guidelines section when nothing has an attribute", () => {
    const s = sectionsFor(17, [entry("a", 2020), entry("b", 2021)]);
    expect(s).toHaveLength(1);
    expect(s[0].title).toBeNull();
  });

  it("leaves other categories alone", () => {
    expect(sectionsFor(5, [entry("a", 2020, ["Meetings"])])).toHaveLength(1);
  });
});

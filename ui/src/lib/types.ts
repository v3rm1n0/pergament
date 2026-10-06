import type { CatalogEntry, Category, PubCard } from "@/lib/api";

/**
 * Library category of a manifest `publicationType`. Ids 1–31 are the catalog's
 * `PublicationTypeId` (CATEGORIES in src/catalog.rs); negative ids are
 * categories the catalog does not have. Curriculum and Outlines only exist
 * once such a publication is imported.
 */
const BY_TYPE: Record<string, { id: number; name: string }> = {
  Bible: { id: 1, name: "Bible" },
  Book: { id: 2, name: "Books" },
  Brochure: { id: 4, name: "Brochures and Booklets" },
  Booklet: { id: 4, name: "Brochures and Booklets" },
  Index: { id: 6, name: "Index" },
  "Kingdom Ministry": { id: 7, name: "Kingdom Ministry" },
  Tract: { id: 10, name: "Tracts and Invitations" },
  Awake: { id: 13, name: "Awake!" },
  "Awake!": { id: 13, name: "Awake!" },
  Watchtower: { id: 14, name: "Watchtower" },
  Guidelines: { id: 17, name: "Guidelines" },
  "Article Series": { id: 22, name: "Article Series" },
  "Meeting Workbook": { id: 30, name: "Meeting Workbooks" },
  Program: { id: 31, name: "Programs" },
  Curriculum: { id: -2, name: "Curriculum" },
  Outline: { id: -3, name: "Outlines" },
};

const OTHER = { id: -4, name: "Other" };

export function categoryOf(pub: Pick<PubCard, "publicationType">): { id: number; name: string } {
  return (pub.publicationType && BY_TYPE[pub.publicationType]) || OTHER;
}

/** Order of the categories of imported publications; unknown ones come last. */
const ORDER = [1, 2, 4, 10, 22, 14, 13, 30, 7, 31, 6, 17, -2, -3, -4];

/** Imported publications grouped by category, in display order. */
export function groupByCategory(pubs: PubCard[]): { id: number; name: string; pubs: PubCard[] }[] {
  const groups = new Map<number, { id: number; name: string; pubs: PubCard[] }>();
  for (const p of pubs) {
    const c = categoryOf(p);
    const g = groups.get(c.id) ?? { ...c, pubs: [] };
    g.pubs.push(p);
    groups.set(c.id, g);
  }
  return [...groups.values()].sort((a, b) => ORDER.indexOf(a.id) - ORDER.indexOf(b.id));
}

/** Catalog categories plus those that only exist through imported publications. */
export function mergeCategories(catalog: Category[], pubs: PubCard[]): Category[] {
  const out = [...catalog];
  for (const g of groupByCategory(pubs)) {
    if (!out.some((c) => c.id === g.id)) out.push({ id: g.id, name: g.name, count: g.pubs.length });
  }
  return out;
}

/** Imported publications of a category that the catalog does not list. */
export function importedOnly(entries: CatalogEntry[], pubs: PubCard[], category: number): CatalogEntry[] {
  const listed = new Set(entries.map((e) => e.local));
  return pubs
    .filter((p) => categoryOf(p).id === category && !listed.has(p.dir))
    .sort((a, b) => b.year - a.year || b.issueTag.localeCompare(a.issueTag))
    .map(
      (p): CatalogEntry => ({
        item: {
          key_symbol: p.symbol,
          symbol: p.symbol,
          meps_language: p.mepsLanguage,
          issue_tag: Number(p.issueTag) || 0,
          year: p.year,
          title: p.title,
          issue_title: null,
          size: 0,
          sha1: "",
          publication_type: category,
          short_title: p.shortTitle,
          cataloged_on: null,
          image: null,
        },
        category: categoryOf(p).name,
        imageUrl: p.cover,
        local: p.dir,
      }),
    );
}

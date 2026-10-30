import type { CatalogEntry, Category, Conflict, PubCard } from "@/lib/api";
import { t } from "@/lib/i18n";

export interface RollingSettings {
  enabled: boolean;
  keep: number;
}
export interface SyncSettings {
  folder: string | null;
  deviceId: string | null;
  /** The host part of this computer's sync file name, kept from when the id was made. */
  host?: string | null;
}
export interface PendingPeer {
  file: string;
  device: string;
  deletions: number;
}
export interface SyncStatus {
  folder: string | null;
  /** UNIX seconds. */
  lastSync: number | null;
  lastDevice: string | null;
  error: string | null;
  pending: PendingPeer[];
  conflicts: Conflict[];
}
export interface AutoSettings {
  rolling: RollingSettings;
  sync: SyncSettings;
}

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
  "Manual/Guidelines": { id: 17, name: "Guidelines" },
  "Article Series": { id: 22, name: "Article Series" },
  "Meeting Workbook": { id: 30, name: "Meeting Workbooks" },
  Program: { id: 31, name: "Programs" },
  Curriculum: { id: -2, name: "Curriculum" },
  Talk: { id: -3, name: "Outlines" },
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
          key_symbol: p.keySymbol,
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
          attributes: guessAttributes(p),
        },
        category: categoryOf(p).name,
        imageUrl: p.cover,
        local: p.dir,
      }),
    );
}

/** Attributes of an imported publication, which has none, guessed from its symbol. */
function guessAttributes(p: PubCard): string[] {
  const s = p.symbol;
  if (/^(yb|syr)\d/.test(s)) return ["Yearbook"];
  if (/^es\d/.test(s)) return ["Examining the Scriptures"];
  if (/^CA-/.test(s)) return ["Circuit Assembly"];
  if (/^CO-/.test(s)) return ["Convention"];
  if (/^kn\d/.test(s)) return ["Kingdom News"];
  if (/^wp/.test(s)) return ["Public"];
  if (s === "w" && p.year >= 2008) return ["Study"];
  return [];
}

export interface Part {
  title: string | null;
  entries: CatalogEntry[];
}

export interface Section {
  title: string | null;
  parts: Part[];
}

const newest = (a: CatalogEntry, b: CatalogEntry) =>
  b.item.year - a.item.year || b.item.issue_tag - a.item.issue_tag;

/** Entries split by the first of `rules` whose attribute they have; the rest go first, untitled. */
function byAttribute(entries: CatalogEntry[], rules: [attr: string, title: string][]): Section[] {
  const buckets: CatalogEntry[][] = [[], ...rules.map(() => [])];
  for (const e of entries) {
    const i = rules.findIndex(([attr]) => e.item.attributes.includes(attr));
    buckets[i + 1].push(e);
  }
  const titles = [null, ...rules.map(([, title]) => t(title))];
  return buckets
    .map((b, i): Section => ({ title: titles[i], parts: [{ title: null, entries: b }] }))
    .filter((s) => s.parts[0].entries.length > 0);
}

/**
 * Entries split by their first attribute, whatever it is; entries without one come first, untitled. The headings are
 * the attribute names, translated where the locales know them.
 */
function byEveryAttribute(entries: CatalogEntry[]): Section[] {
  const buckets = new Map<string | null, CatalogEntry[]>();
  for (const e of entries) {
    const key = e.item.attributes[0] ?? null;
    buckets.set(key, [...(buckets.get(key) ?? []), e]);
  }
  const titles = [...buckets.keys()]
    .filter((k): k is string => k !== null)
    .map((k) => [k, t(k)] as const)
    .sort(([, a], [, b]) => a.localeCompare(b));
  return [[null, null] as const, ...titles].flatMap(([key, title]): Section[] => {
    const list = buckets.get(key);
    return list ? [{ title, parts: [{ title: null, entries: list }] }] : [];
  });
}

/**
 * A category page split the way the original app does it. The newer
 * publications come first and carry no heading.
 */
export function sectionsFor(category: number, all: CatalogEntry[]): Section[] {
  const entries = [...all].sort(newest);
  switch (category) {
    case 2:
      return byAttribute(entries, [
        ["Yearbook", "Yearbooks"],
        ["Archive", "Older Publications"],
      ]);
    case 4:
      return byAttribute(entries, [
        ["Examining the Scriptures", "Examining the Scriptures"],
        ["Archive", "Older Publications"],
      ]);
    case 10:
      return byAttribute(entries, [
        ["Convention", "Convention"],
        ["Archive", "Older Publications"],
        ["Kingdom News", "Kingdom News"],
      ]);
    case 31:
      return byAttribute(entries, [
        ["Circuit Assembly", "Circuit Assembly"],
        ["Convention", "Convention Program"],
      ]);
    case 13:
    case 14: {
      const years = [...new Set(entries.map((e) => e.item.year))];
      return years.map((year) => {
        const ofYear = entries.filter((e) => e.item.year === year);
        const part = (title: string | null, attr: string | null): Part => ({
          title,
          entries: ofYear.filter((e) =>
            attr ? e.item.attributes.includes(attr) : !e.item.attributes.some((a) => a === "Public" || a === "Study"),
          ),
        });
        const parts = [part(t("Public Edition"), "Public"), part(t("Study Edition"), "Study"), part(null, null)];
        return { title: String(year), parts: parts.filter((p) => p.entries.length > 0) };
      });
    }
    case 17:
      return byEveryAttribute(entries);
    default:
      return [{ title: null, parts: [{ title: null, entries }] }];
  }
}

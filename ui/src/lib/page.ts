/** A rendered page split into reading text and study-pane content. */
export interface SplitPage {
  /** HTML of the reading text without the extra sections. */
  body: string;
  /** `footnoteN` -> HTML. */
  footnotes: Record<string, string>;
  /** `xrefN` -> HTML (list of references). */
  xrefs: Record<string, string>;
  /** `book:chapter:verse` -> study note HTML, in order. */
  notes: Record<string, string[]>;
  /** Verse keys in reading order that have notes. */
  noteOrder: string[];
}

const SECTIONS = "section.footnotes, section.xrefs, section.study-notes, div.groupFootnote";

/** `pergament://bible/40:1:1-40:1:1` -> `40:1:1`. */
export function verseKeyFromHref(href: string): string | null {
  const m = /^pergament:\/\/bible\/(\d+):(\d+):(\d+)/.exec(href);
  return m ? `${m[1]}:${m[2]}:${m[3]}` : null;
}

/** `v40-1-1-1` or `v40-1-1` -> `40:1:1`. */
export function verseKeyFromId(id: string): string | null {
  const m = /^v(\d+)-(\d+)-(\d+)(?:-\d+)?$/.exec(id);
  return m ? `${m[1]}:${m[2]}:${m[3]}` : null;
}

export function splitPage(html: string): SplitPage {
  const doc = new DOMParser().parseFromString(`<div id="root">${html}</div>`, "text/html");
  const root = doc.getElementById("root")!;
  const footnotes: Record<string, string> = {};
  const xrefs: Record<string, string> = {};
  const notes: Record<string, string[]> = {};
  const noteOrder: string[] = [];

  root.querySelectorAll<HTMLElement>("[id^=footnote]").forEach((el) => {
    if (!/^footnote\d+$/.test(el.id)) return;
    // Bible footnotes are wrapped with their letter; prefer the wrapper.
    const wrapper = el.parentElement?.classList.contains("footnote") ? el.parentElement : el;
    footnotes[el.id] = wrapper.innerHTML;
  });
  root.querySelectorAll<HTMLElement>("p.xref[id^=xref]").forEach((el) => {
    xrefs[el.id] = el.innerHTML;
  });
  root.querySelectorAll<HTMLElement>("div.study-note").forEach((el) => {
    const href = el.querySelector("a[href^='pergament://bible/']")?.getAttribute("href");
    const key = href ? verseKeyFromHref(href) : null;
    if (!key) return;
    if (!notes[key]) {
      notes[key] = [];
      noteOrder.push(key);
    }
    notes[key].push(el.innerHTML);
  });

  root.querySelectorAll(SECTIONS).forEach((el) => el.remove());
  return { body: root.innerHTML, footnotes, xrefs, notes, noteOrder };
}

/** Plain text of the theme scripture of a daily text. */
export function themeScripture(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return (doc.querySelector(".themeScrp")?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** The theme scripture split into its text and the reference to it, which is the last link inside it. */
export function themeScriptureParts(html: string): { text: string; reference: string } {
  const el = new DOMParser().parseFromString(html, "text/html").querySelector(".themeScrp");
  if (!el) return { text: "", reference: "" };
  const clean = (s: string | null) => (s ?? "").replace(/\s+/g, " ").trim();
  const links = el.querySelectorAll("a");
  const link = links[links.length - 1];
  const reference = clean(link?.textContent ?? null);
  link?.remove();
  // The reference stands in brackets after the text; without the link they are left empty or open.
  const text = clean(el.textContent)
    .replace(/[\s(\[]*[)\]]*$/, "")
    .replace(/[\s–—-]+$/, "");
  return { text, reference: reference.replace(/^[\s(\[]+|[\s)\]]+$/g, "") };
}

/** What a link cites inside the page it opens. */
export type Cited =
  | { kind: "verses"; from: [number, number]; to: [number, number] }
  | { kind: "paragraphs"; from: number; to: number };

/**
 * `pergament://bible/58:13:7-58:13:9` and `pergament://verse/X:1/13:7-13:9` cite
 * verses (chapter, verse), `pergament://pub/X:2024366/7-8` cites paragraphs.
 */
export function citedBy(href: string): Cited | null {
  const bible = /^pergament:\/\/bible\/\d+:(\d+):(\d+)(?:-\d+:(\d+):(\d+))?/.exec(href);
  const verse = /^pergament:\/\/(?:verse|note)\/[^/]+\/(\d+):(\d+)(?:-(\d+):(\d+))?/.exec(href);
  const m = bible ?? verse;
  if (m) {
    const from: [number, number] = [Number(m[1]), Number(m[2])];
    return { kind: "verses", from, to: m[3] ? [Number(m[3]), Number(m[4])] : from };
  }
  const pub = /^pergament:\/\/pub\/[^/]+\/(\d+)(?:-(\d+))?/.exec(href);
  if (pub) return { kind: "paragraphs", from: Number(pub[1]), to: Number(pub[2] ?? pub[1]) };
  return null;
}

/** Mark the cited verses or paragraphs under `root`; returns the first one. */
export function markCited(root: Element, cited: Cited | null): Element | null {
  if (!cited) return null;
  const marked: Element[] = [];
  if (cited.kind === "verses") {
    const key = (c: number, v: number) => c * 1000 + v;
    const [lo, hi] = [key(...cited.from), key(...cited.to)];
    root.querySelectorAll("span.v[id]").forEach((el) => {
      const m = /^v\d+-(\d+)-(\d+)/.exec(el.id);
      const k = m ? key(Number(m[1]), Number(m[2])) : -1;
      if (k >= lo && k <= hi) marked.push(el);
    });
  } else {
    for (let n = cited.from; n <= cited.to; n++) {
      const el = root.querySelector(`[id="p${n}"]`);
      if (el) marked.push(el);
    }
  }
  marked.forEach((el) => el.classList.add("cited"));
  return marked[0] ?? null;
}

/**
 * Only the cited paragraphs of a page, or `null` when none of them is found
 * (the whole page is shown then).
 */
export function extractParagraphs(html: string, cited: Extract<Cited, { kind: "paragraphs" }>): string | null {
  const doc = new DOMParser().parseFromString(`<div id="root">${html}</div>`, "text/html");
  const found: Element[] = [];
  for (let n = cited.from; n <= cited.to; n++) {
    const el = doc.querySelector(`[id="p${n}"]`);
    if (el && !found.some((f) => f.contains(el))) found.push(el);
  }
  return found.length > 0 ? found.map((el) => el.outerHTML).join("") : null;
}

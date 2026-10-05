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

/** `jwlinux://bible/40:1:1-40:1:1` -> `40:1:1`. */
export function verseKeyFromHref(href: string): string | null {
  const m = /^jwlinux:\/\/bible\/(\d+):(\d+):(\d+)/.exec(href);
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
    const href = el.querySelector("a[href^='jwlinux://bible/']")?.getAttribute("href");
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

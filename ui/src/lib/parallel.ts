/** Verses a Bible link cites: book, then (chapter, verse) from and to. */
export interface BibleRange {
  book: number;
  from: [number, number];
  to: [number, number];
}

/** `pergament://bible/19:68:5-19:68:7`; a bare chapter (`1:2`) cites the whole chapter. */
export function parseBibleRange(href: string): BibleRange | null {
  const m = /^pergament:\/\/bible\/(\d+):(\d+)(?::(\d+))?(?:-(\d+):(\d+)(?::(\d+))?)?/.exec(href);
  if (!m) return null;
  const book = Number(m[1]);
  const from: [number, number] = [Number(m[2]), Number(m[3] ?? 1)];
  if (m[3] === undefined) return { book, from, to: [from[0], 9999] };
  const to: [number, number] = m[4] ? [Number(m[5]), Number(m[6] ?? m[5])] : from;
  return { book, from, to };
}

/** "Hebrews 10:1", "Hebrews 10:1-3" or "Hebrews 10:39–11:2". */
export function rangeText(bookTitle: string, r: BibleRange): string {
  const [[c1, v1], [c2, v2]] = [r.from, r.to];
  if (v2 === 9999) return `${bookTitle} ${c1}`;
  if (c1 !== c2) return `${bookTitle} ${c1}:${v1}–${c2}:${v2}`;
  return v1 === v2 ? `${bookTitle} ${c1}:${v1}` : `${bookTitle} ${c1}:${v1}-${v2}`;
}

/**
 * Only the cited verses of rendered chapter HTML, without footnote and
 * cross-reference markers or links.
 */
export function extractVerses(html: string, r: BibleRange): string {
  const doc = new DOMParser().parseFromString(`<div id="root">${html}</div>`, "text/html");
  const key = (c: number, v: number) => c * 10000 + v;
  const [lo, hi] = [key(...r.from), key(...r.to)];
  const out: string[] = [];
  doc.querySelectorAll<HTMLElement>("span.v[id]").forEach((el) => {
    const m = /^v(\d+)-(\d+)-(\d+)/.exec(el.id);
    if (!m || Number(m[1]) !== r.book) return;
    const k = key(Number(m[2]), Number(m[3]));
    if (k < lo || k > hi) return;
    const copy = el.cloneNode(true) as HTMLElement;
    copy.querySelectorAll("a.fn, a.xr, sup").forEach((x) => x.remove());
    copy.querySelectorAll("a").forEach((a) => a.replaceWith(...Array.from(a.childNodes)));
    out.push(copy.outerHTML);
  });
  return out.join(" ");
}

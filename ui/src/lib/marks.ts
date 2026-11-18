// Highlights in rendered pages. User data addresses text by block (a
// paragraph `p{pid}` or a Bible verse) and token index within the block, see
// docs/FORMAT.md ("User data").
import type { Mark, MarkRange } from "./api";

export const BLOCK_PARAGRAPH = 1;
export const BLOCK_VERSE = 2;

/** Highlight colors by `ColorIndex`. */
export const HIGHLIGHT_COLORS: { index: number; name: string }[] = [
  { index: 1, name: "Yellow" },
  { index: 2, name: "Green" },
  { index: 3, name: "Blue" },
  { index: 4, name: "Pink" },
  { index: 5, name: "Orange" },
  { index: 6, name: "Purple" },
];

/** Verse and paragraph numbers and footnote or reference markers are not part of the text. */
const SKIP = ".note-dot, .vl, .cl, a.fn, a.xr, .pageNum, .parNum";

/**
 * A word, a number or a single punctuation character. `-` and `:` between word
 * characters stay inside the token (`Neue-Welt-Übersetzung`, `22:1-3`) and soft
 * hyphens are not counted, which is how the stored highlights are numbered.
 */
const TOKEN = /[\p{L}\p{N}\p{M}]+(?:[-\u2010\u2011:][\p{L}\p{N}\p{M}]+)*|[^\s\p{L}\p{N}\p{M}\u00ad]/gu;

interface Token {
  node: Text;
  start: number;
  end: number;
}

/** Elements of one block: the parts of a verse, or a paragraph. */
export function blockElements(root: Element, blockType: number, identifier: number, chapter?: number): Element[] {
  if (blockType === BLOCK_VERSE) {
    return [...root.querySelectorAll("span.v[id]")].filter((el) => {
      const m = /^v\d+-(\d+)-(\d+)-\d+$/.exec(el.id);
      return m !== null && Number(m[2]) === identifier && (chapter === undefined || Number(m[1]) === chapter);
    });
  }
  const el = root.querySelector(`[data-pid="${identifier}"]`) ?? root.querySelector(`[id="p${identifier}"]`);
  return el ? [el] : [];
}

/** The block an element belongs to. */
export function blockOf(el: Element | null): { blockType: number; identifier: number; element: Element } | null {
  const verse = el?.closest("span.v[id]");
  const vm = verse ? /^v\d+-\d+-(\d+)-\d+$/.exec(verse.id) : null;
  if (verse && vm) return { blockType: BLOCK_VERSE, identifier: Number(vm[1]), element: verse };
  const para = el?.closest("[data-pid], [id^=p]");
  const pid = para?.getAttribute("data-pid");
  const identifier = pid && /^\d+$/.test(pid) ? pid : para ? /^p(\d+)$/.exec(para.id)?.[1] : null;
  if (para && identifier) return { blockType: BLOCK_PARAGRAPH, identifier: Number(identifier), element: para };
  return null;
}

/** Tokens of a block in reading order. */
export function tokens(elements: Element[]): Token[] {
  const out: Token[] = [];
  for (const el of elements) {
    const walker = el.ownerDocument.createTreeWalker(el, NodeFilter.SHOW_TEXT, {
      acceptNode: (n) => (n.parentElement?.closest(SKIP) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
    });
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      const text = n as Text;
      for (const m of text.data.matchAll(TOKEN)) {
        out.push({ node: text, start: m.index, end: m.index + m[0].length });
      }
    }
  }
  return out;
}

/** Wrap tokens `start..=end` of a block in `<mark>` elements. */
function wrap(root: Element, range: MarkRange, mark: Mark, chapter?: number) {
  const toks = tokens(blockElements(root, range.blockType, range.identifier, chapter)).slice(range.start, range.end + 1);
  // One span of text per node, wrapped from the back so offsets stay valid.
  const spans = new Map<Text, [number, number]>();
  for (const t of toks) {
    const s = spans.get(t.node);
    spans.set(t.node, s ? [s[0], t.end] : [t.start, t.end]);
  }
  for (const [node, [start, end]] of [...spans].reverse()) {
    const doc = node.ownerDocument;
    const tail = node.splitText(start);
    tail.splitText(end - start);
    const el = doc.createElement("mark");
    el.className = "hl";
    el.dataset.color = String(mark.color);
    el.dataset.guid = mark.guid;
    tail.replaceWith(el);
    el.append(tail);
  }
}

/** `chapter` limits verse highlights to that chapter, for text that spans several. */
export function applyMarks(root: Element, marks: Mark[], chapter?: number) {
  for (const mark of marks) for (const r of mark.ranges) wrap(root, r, mark, chapter);
}

/** Token ranges covered by a selection inside `root`. */
export function selectionRanges(root: Element, range: Range): MarkRange[] {
  const blocks = new Map<string, { blockType: number; identifier: number }>();
  const candidates = root.querySelectorAll("span.v[id], [data-pid], [id^=p]");
  for (const el of candidates) {
    if (!range.intersectsNode(el)) continue;
    const b = blockOf(el);
    // Bible paragraphs hold verses; take the verses, not the paragraph.
    if (!b || (b.blockType === BLOCK_PARAGRAPH && el.querySelector("span.v[id]"))) continue;
    blocks.set(`${b.blockType}:${b.identifier}`, { blockType: b.blockType, identifier: b.identifier });
  }
  const out: MarkRange[] = [];
  for (const { blockType, identifier } of blocks.values()) {
    const toks = tokens(blockElements(root, blockType, identifier));
    const picked = toks
      .map((t, i) => ({ t, i }))
      .filter(({ t }) => overlaps(range, t));
    if (picked.length > 0) {
      out.push({ blockType, identifier, start: picked[0].i, end: picked[picked.length - 1].i });
    }
  }
  return out;
}

function overlaps(range: Range, t: Token): boolean {
  const endsAfterStart =
    range.comparePoint(t.node, t.end) >= 0 && !(t.node === range.startContainer && t.end === range.startOffset);
  const startsBeforeEnd =
    range.comparePoint(t.node, t.start) <= 0 && !(t.node === range.endContainer && t.start === range.endOffset);
  return endsAfterStart && startsBeforeEnd;
}

/** Text of a selection without verse numbers and markers, as one line. */
export function rangeText(range: Range): string {
  const box = document.createElement("div");
  box.append(range.cloneContents());
  box.querySelectorAll(SKIP).forEach((el) => el.remove());
  return box.textContent?.replace(/\s+/g, " ").trim() ?? "";
}

/** Text of the highlight with this guid. */
export function markText(root: Element, guid: string): string {
  const parts = [...root.querySelectorAll<HTMLElement>("mark.hl")].filter((m) => m.dataset.guid === guid);
  return parts.map((m) => m.textContent ?? "").join(" ").replace(/\s+/g, " ").trim();
}

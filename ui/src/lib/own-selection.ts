/** Characters that make up a word, so a press selects `don't` or `Gott-Vater` whole. */
const WORD = /[\p{L}\p{N}\p{M}'’-]/u;

function caretAt(x: number, y: number): { node: Node; offset: number } | null {
  const range = document.caretRangeFromPoint(x, y);
  return range ? { node: range.startContainer, offset: range.startOffset } : null;
}

/** The word under a point as a range, or null when the point is not on a word's text. */
export function wordAt(x: number, y: number): Range | null {
  const caret = caretAt(x, y);
  if (!caret || caret.node.nodeType !== Node.TEXT_NODE) return null;
  const text = caret.node.textContent ?? "";
  let start = caret.offset;
  let end = caret.offset;
  while (start > 0 && WORD.test(text[start - 1])) start--;
  while (end < text.length && WORD.test(text[end])) end++;
  if (start === end) return null;
  const range = document.createRange();
  range.setStart(caret.node, start);
  range.setEnd(caret.node, end);
  return range;
}

/**
 * Moves the moving end of the selection to the word under a point. The selection keeps the word it started on;
 * past that word it reaches the word's end, before it its start, so whole words stay selected.
 */
export function extendTo(x: number, y: number): void {
  const sel = window.getSelection();
  const word = wordAt(x, y);
  if (!sel || sel.rangeCount === 0 || !word || !sel.anchorNode) return;
  const anchor = document.createRange();
  anchor.setStart(sel.anchorNode, sel.anchorOffset);
  anchor.collapse(true);
  if (anchor.comparePoint(word.endContainer, word.endOffset) >= 0) sel.extend(word.endContainer, word.endOffset);
  else sel.extend(word.startContainer, word.startOffset);
}

/** Selects the word under a point, replacing the selection. */
export function selectWordAt(x: number, y: number): boolean {
  const word = wordAt(x, y);
  const sel = window.getSelection();
  if (!word || !sel) return false;
  sel.removeAllRanges();
  sel.addRange(word);
  return true;
}

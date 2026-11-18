import { describe, expect, it } from "vitest";
import { applyMarks, blockElements, blockOf, BLOCK_PARAGRAPH, BLOCK_VERSE, selectionRanges } from "./marks";

const verse = (c: number, v: number) => `<span class="v" id="v46-${c}-${v}-1">Text ${c}:${v}</span>`;
const mark = { guid: "g", color: 1, ranges: [{ blockType: BLOCK_VERSE, identifier: 12, start: 0, end: 1 }] };

describe("applyMarks", () => {
  it("marks only the given chapter, for text that spans several", () => {
    const box = document.createElement("div");
    box.innerHTML = verse(12, 12) + verse(13, 12);
    applyMarks(box, [mark], 12);
    const marked = [...box.querySelectorAll("mark.hl")].map((m) => m.closest("span.v")?.id);
    expect(marked).toEqual(["v46-12-12-1"]);
    applyMarks(box, [mark], 13);
    expect(box.querySelectorAll("mark.hl")).toHaveLength(2);
  });

  describe("paragraph addresses", () => {
    it.each([
      'id="p12"',
      'data-pid="12"',
      'id="other" data-pid="12"',
      'id="p99" data-pid="12"',
    ])("uses the same paragraph for clicks, selections and marks with %s", (attributes) => {
      const root = document.createElement("div");
      root.innerHTML = `<p ${attributes}><strong>Some words</strong></p>`;
      const paragraph = root.firstElementChild!;
      const text = paragraph.firstElementChild!;
      expect(blockOf(text)).toEqual({ blockType: BLOCK_PARAGRAPH, identifier: 12, element: paragraph });
      expect(blockElements(root, BLOCK_PARAGRAPH, 12)).toEqual([paragraph]);
      const range = document.createRange();
      range.selectNodeContents(text);
      const ranges = selectionRanges(root, range);
      expect(ranges).toEqual([{ blockType: BLOCK_PARAGRAPH, identifier: 12, start: 0, end: 1 }]);
      applyMarks(root, [{ guid: "p", color: 2, ranges }]);
      expect(root.querySelector("mark.hl")?.textContent).toBe("Some words");
    });
  });
});

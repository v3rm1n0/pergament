import { describe, expect, it } from "vitest";
import { extractVerses, parseBibleRange, rangeText } from "@/lib/parallel";

describe("parallel translations", () => {
  it("parses Bible links", () => {
    expect(parseBibleRange("pergament://bible/58:10:1-58:10:1")).toEqual({ book: 58, from: [10, 1], to: [10, 1] });
    expect(parseBibleRange("pergament://bible/19:68:5-19:68:7")?.to).toEqual([68, 7]);
    expect(parseBibleRange("pergament://bible/1:2")).toEqual({ book: 1, from: [2, 1], to: [2, 9999] });
    expect(parseBibleRange("pergament://pub/X:1/1")).toBeNull();
  });

  it("labels ranges", () => {
    expect(rangeText("Hebrews", { book: 58, from: [10, 1], to: [10, 1] })).toBe("Hebrews 10:1");
    expect(rangeText("Hebrews", { book: 58, from: [10, 1], to: [10, 3] })).toBe("Hebrews 10:1-3");
    expect(rangeText("Hebrews", { book: 58, from: [10, 39], to: [11, 2] })).toBe("Hebrews 10:39–11:2");
  });

  it("keeps only the cited verses, without markers or links", () => {
    const html =
      '<p><span class="v" id="v58-10-1-1"><span class="vl">1</span>Das Gesetz<a class="fn" href="#footnote1">a</a> <a href="x">ist</a></span> ' +
      '<span class="v" id="v58-10-2-1">Zwei</span></p>';
    const out = extractVerses(html, { book: 58, from: [10, 1], to: [10, 1] });
    expect(out).toContain("Das Gesetz");
    expect(out).toContain("ist");
    expect(out).not.toContain("Zwei");
    expect(out).not.toContain("footnote");
    expect(out).not.toContain("<a");
  });
});

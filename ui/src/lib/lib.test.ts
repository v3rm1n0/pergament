import { describe, expect, it } from "vitest";
import { bookShade, booksTabIndex, shortBookName } from "./bible";
import { currentView, initialNav, navReducer } from "./nav";
import { citedBy, markCited, splitPage, themeScripture, verseKeyFromHref, verseKeyFromId } from "./page";
import { defaultLangCode, prefersDark } from "./settings";
import { BLOCK_PARAGRAPH, BLOCK_VERSE, applyMarks, blockElements, blockOf, selectionRanges, tokens } from "./marks";
import type { TocNode } from "./api";

const node = (title: string, children: TocNode[] = [], bible_book: number | null = null): TocNode => ({
  title,
  document_id: null,
  bible_book,
  children,
});

describe("bible", () => {
  it("shades books by group", () => {
    expect(bookShade(1)).toBe(0);
    expect(bookShade(6)).toBe(2);
    expect(bookShade(19)).toBe(1);
    expect(bookShade(23)).toBe(0);
    expect(bookShade(44)).toBe(2);
    expect(bookShade(45)).toBe(1);
    expect(bookShade(66)).toBe(0);
  });
  it("shortens book names", () => {
    expect(shortBookName("1. Mose (Genesis)")).toBe("1. Mose");
    expect(shortBookName("Psalmen")).toBe("Psalmen");
  });
  it("finds the books tab", () => {
    const toc = [node("EINFÜHRUNG"), node("BÜCHER", [node("HEBR", [node("1. Mose", [], 1)])]), node("INDEX")];
    expect(booksTabIndex(toc)).toBe(1);
    expect(booksTabIndex([node("A")])).toBe(0);
  });
});

describe("page", () => {
  it("parses verse keys", () => {
    expect(verseKeyFromHref("jwlinux://bible/40:1:1-40:1:1")).toBe("40:1:1");
    expect(verseKeyFromHref("https://x")).toBeNull();
    expect(verseKeyFromId("v40-1-2-1")).toBe("40:1:2");
    expect(verseKeyFromId("v19-3-0")).toBe("19:3:0");
    expect(verseKeyFromId("p3")).toBeNull();
  });

  it("splits sections from the reading text", () => {
    const html = `<article><p><span id="v40-1-1-1" class="v">Text<a class="fn" href="#footnote1">a</a><a class="xr" href="#xref2">b</a></span></p></article>
      <section class="footnotes"><h2>Footnotes</h2><div class="footnote"><a class="fn-back" href="#footnotesource1">a</a><div id="footnote1"><p>Or "x".</p></div></div></section>
      <section class="xrefs"><h2>Cross references</h2><p class="xref" id="xref2"><a href="#xrefsource2">b</a> <a class="b" href="jwlinux://bible/1:1:1-1:1:1">1. Mose 1:1</a></p></section>
      <section class="study-notes"><h2>Study notes</h2><div class="study-note" id="note23261"><p><a href="jwlinux://bible/40:1:1-40:1:1"><strong>1:1</strong></a></p><p>Note</p></div></section>`;
    const s = splitPage(html);
    expect(s.body).toContain("Text");
    expect(s.body).not.toContain("Footnotes");
    expect(s.body).not.toContain("Note</p>");
    expect(s.footnotes.footnote1).toContain("fn-back");
    expect(s.footnotes.footnote1).toContain("Or");
    expect(s.xrefs.xref2).toContain("1. Mose 1:1");
    expect(s.notes["40:1:1"]).toHaveLength(1);
    expect(s.noteOrder).toEqual(["40:1:1"]);
  });

  it("handles inline document footnotes", () => {
    const s = splitPage(`<p>x<a class="fn" href="#footnote1">a</a></p><div class="groupFootnote"><div id="footnote1" class="fn-ref"><p>Fn</p></div></div>`);
    expect(s.footnotes.footnote1).toContain("Fn");
    expect(s.body).not.toContain("Fn");
  });
});

describe("nav", () => {
  it("pushes and pops", () => {
    let s = navReducer(initialNav, { type: "push", view: { name: "online" } });
    expect(currentView(s).name).toBe("online");
    s = navReducer(s, { type: "back" });
    expect(currentView(s).name).toBe("home");
    expect(navReducer(s, { type: "back" })).toBe(s);
    s = navReducer(s, { type: "root", view: { name: "settings" } });
    expect(s.stack).toHaveLength(1);
  });
});

describe("settings", () => {
  it("resolves theme", () => {
    expect(prefersDark("system", true)).toBe(true);
    expect(prefersDark("light", true)).toBe(false);
    expect(prefersDark("dark", false)).toBe(true);
  });
  it("maps locales to catalog codes", () => {
    expect(defaultLangCode("de-DE")).toBe("X");
    expect(defaultLangCode("en-US")).toBe("E");
    expect(defaultLangCode("pt-BR")).toBe("E");
  });
});

import { inLanguage } from "./settings";

describe("language filter", () => {
  it("keeps the selected language and unknown entries", () => {
    const items = [{ langCode: "X" }, { langCode: "E" }, { langCode: null }];
    expect(inLanguage(items, "X")).toEqual([{ langCode: "X" }, { langCode: null }]);
  });
});

import { addDays, ago, fromDateNumber, isoDate, longDate, rangeLabel, weekLabel, weekStart } from "./dates";

describe("dates", () => {
  it("finds the meeting week", () => {
    const wed = new Date(2026, 9, 7); // 2026-10-07, a Wednesday
    expect(isoDate(weekStart(wed))).toBe("2026-10-05");
    expect(isoDate(weekStart(new Date(2026, 9, 11)))).toBe("2026-10-05"); // Sunday
    expect(weekLabel(weekStart(wed))).toBe("October 5-11");
    expect(weekLabel(new Date(2026, 8, 28))).toBe("September 28 - October 4");
    expect(isoDate(addDays(new Date(2026, 9, 5), 7))).toBe("2026-10-12");
  });
  it("formats ages and ranges", () => {
    const now = new Date("2026-10-05T12:00:00Z");
    expect(ago("2026-10-04T07:21:15+00:00", now)).toBe("1 day ago");
    expect(ago("2026-10-03T07:21:15+00:00", now)).toBe("2 days ago");
    expect(ago(null, now)).toBe("");
    expect(rangeLabel("2026-10-05", "2026-11-01")).toBe("October 5 to November 1");
  });
});

describe("references", () => {
  it("reads what a link cites", () => {
    expect(citedBy("jwlinux://bible/58:13:7-58:13:9")).toEqual({ kind: "verses", from: [13, 7], to: [13, 9] });
    expect(citedBy("jwlinux://bible/19:23:1")).toEqual({ kind: "verses", from: [23, 1], to: [23, 1] });
    expect(citedBy("jwlinux://verse/X:1001070105/1:1-1:31")).toEqual({ kind: "verses", from: [1, 1], to: [1, 31] });
    expect(citedBy("jwlinux://pub/X:2024366/7-8")).toEqual({ kind: "paragraphs", from: 7, to: 8 });
    expect(citedBy("jwlinux://pub/X:2026520/")).toBeNull();
    expect(citedBy("https://www.jw.org/")).toBeNull();
  });
  it("marks cited verses and paragraphs", () => {
    const doc = new DOMParser().parseFromString(
      `<div><span class="v" id="v58-13-6-1">a</span><span class="v" id="v58-13-7-1">b</span><span class="v" id="v58-13-8-1">c</span>
       <p id="p6">x</p><p id="p7">y</p></div>`,
      "text/html",
    );
    const root = doc.body.firstElementChild!;
    expect(markCited(root, citedBy("jwlinux://bible/58:13:7-58:13:8"))?.id).toBe("v58-13-7-1");
    expect([...root.querySelectorAll(".cited")].map((e) => e.id)).toEqual(["v58-13-7-1", "v58-13-8-1"]);
    expect(markCited(root, citedBy("jwlinux://pub/X:1/7-7"))?.id).toBe("p7");
  });
  it("extracts the theme scripture", () => {
    const html = `<header><h2>Mittwoch</h2></header><p class="themeScrp"><em>Denkt an die </em><a><em>Heb. 13:7</em></a></p>`;
    expect(themeScripture(html)).toBe("Denkt an die Heb. 13:7");
    expect(themeScripture("<p>x</p>")).toBe("");
  });
});

describe("dates", () => {
  it("converts date numbers", () => {
    expect(isoDate(fromDateNumber(20261007))).toBe("2026-10-07");
    expect(longDate(fromDateNumber(20261005))).toBe("Monday, October 5");
  });
});

describe("marks", () => {
  // Psalm 23:1, 4 as rendered: verse spans, chapter and verse numbers, markers.
  const html = `<div id="root">
    <p id="p417"><span class="v" id="v19-23-1-1"><span class="cl"><strong>23</strong> </span>Jehova ist mein Hirte.<a class="xr" href="#xref1">a</a></span></p>
    <p id="p418"><span class="v" id="v19-23-1-2">Mir wird nichts fehlen.</span></p>
    <p id="p422"><span class="v" id="v19-23-4-1"> <span class="vl">4 </span>Geht es auch durch das Tal dunklen Schattens,</span></p>
    <p id="p9">Wir können nicht erwarten, dass Jehova uns (<a href="#x"><em>cl</em> 72</a>)</p>
  </div>`;
  const load = () => new DOMParser().parseFromString(html, "text/html").getElementById("root")!;

  it("tokenizes blocks like the stored highlights", () => {
    const root = load();
    const words = (b: number, id: number) =>
      tokens(blockElements(root, b, id)).map((t) => t.node.data.slice(t.start, t.end));
    expect(words(BLOCK_VERSE, 1)).toEqual(["Jehova", "ist", "mein", "Hirte", ".", "Mir", "wird", "nichts", "fehlen", "."]);
    expect(words(BLOCK_VERSE, 4).slice(0, 2)).toEqual(["Geht", "es"]);
    expect(words(BLOCK_PARAGRAPH, 9).slice(3, 6)).toEqual(["erwarten", ",", "dass"]);
  });

  it("wraps highlighted tokens across verse parts", () => {
    const root = load();
    applyMarks(root, [{ guid: "g", color: 3, ranges: [{ blockType: BLOCK_VERSE, identifier: 1, start: 3, end: 6 }] }]);
    const marks = [...root.querySelectorAll("mark.hl")];
    expect(marks.map((m) => m.textContent)).toEqual(["Hirte.", "Mir wird"]);
    expect((marks[0] as HTMLElement).dataset.color).toBe("3");
    // Highlighting does not change the token count.
    expect(tokens(blockElements(root, BLOCK_VERSE, 1)).length).toBe(10);
  });

  it("turns a selection into token ranges", () => {
    const root = load();
    const doc = root.ownerDocument;
    const first = root.querySelector("#v19-23-1-1")!.childNodes[1] as Text; // "Jehova ist mein Hirte."
    const second = root.querySelector("#v19-23-1-2")!.firstChild as Text; // "Mir wird nichts fehlen."
    const range = doc.createRange();
    range.setStart(first, 7); // "ist"
    range.setEnd(second, 3); // after "Mir"
    expect(selectionRanges(root, range)).toEqual([{ blockType: BLOCK_VERSE, identifier: 1, start: 1, end: 5 }]);
    expect(blockOf(first.parentElement)?.identifier).toBe(1);
  });
});

import { describe, expect, it } from "vitest";
import { bookShade, booksTabIndex, shortBookName } from "./bible";
import { currentView, initialNav, navReducer } from "./nav";
import { splitPage, verseKeyFromHref, verseKeyFromId } from "./page";
import { defaultLangCode, prefersDark } from "./settings";
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

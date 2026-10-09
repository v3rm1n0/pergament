import { describe, expect, it } from "vitest";
import { locales } from "@/locales";

const markers = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join();

// These checks run over every language in the registry, so a contributed translation is checked as it is added.
describe.each(locales)("locale $code", (l) => {
  it("has a code, a name and a language tag the browser accepts", () => {
    expect(l.code).toMatch(/^[a-z]{2,3}(-[A-Za-z0-9]+)*$/);
    expect(l.name.trim()).not.toBe("");
    expect(() => Intl.getCanonicalLocales(l.tag)).not.toThrow();
  });

  it("keeps the {placeholders} of every text", () => {
    // An empty text is not translated yet and stays English.
    for (const [en, text] of Object.entries(l.texts)) if (text !== "") expect(markers(text), en).toBe(markers(en));
  });

  it("keeps the leading and trailing spaces of the English text", () => {
    // Some texts are fragments such as " from {device}", where the space belongs to the text.
    const edges = (s: string) => [s.match(/^\s*/)![0], s.match(/\s*$/)![0]];
    for (const [en, text] of Object.entries(l.texts)) if (text !== "") expect(edges(text), en).toEqual(edges(en));
  });
});

describe("registry", () => {
  it("has each code once, ignoring case", () => {
    const codes = locales.map((l) => l.code.toLowerCase());
    expect(new Set(codes).size).toBe(codes.length);
    expect(codes).not.toContain("en");
  });
});

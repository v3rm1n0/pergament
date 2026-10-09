import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@/locales", () => ({
  locales: [
    { code: "fr", name: "Français", tag: "fr-FR", texts: { Save: "Enregistrer", Open: "", "{n} days ago": "il y a {n} jours" } },
    { code: "pt-BR", name: "Português (Brasil)", tag: "pt-BR", texts: { Save: "Salvar" } },
    { code: "pt", name: "Português", tag: "pt-PT", texts: { Save: "Guardar" } },
  ],
}));

import { isUiLang, locale, resolveUiLang, setUiLang, t, uiLanguages } from "@/lib/i18n";
import { weekLabel } from "@/lib/dates";

afterEach(() => setUiLang("en"));

describe("resolving the interface language", () => {
  it("prefers the exact tag, then the language alone, else English", () => {
    expect(resolveUiLang("system", "pt-BR")).toBe("pt-BR");
    expect(resolveUiLang("system", "pt-PT")).toBe("pt");
    expect(resolveUiLang("system", "fr-CA")).toBe("fr");
    expect(resolveUiLang("system", "ja-JP")).toBe("en");
    expect(resolveUiLang("system", "")).toBe("en");
  });

  it("ignores case, like browsers do not", () => {
    expect(resolveUiLang("system", "PT-br")).toBe("pt-BR");
    expect(resolveUiLang("system", "FR")).toBe("fr");
  });

  it("takes a chosen language as it is, and English for one that no longer exists", () => {
    expect(resolveUiLang("fr", "de-DE")).toBe("fr");
    expect(resolveUiLang("en", "fr-FR")).toBe("en");
    expect(resolveUiLang("xx", "fr-FR")).toBe("en");
  });

  it("knows which codes are interface languages", () => {
    expect(["en", "fr", "pt-BR", "FR"].map(isUiLang)).toEqual([true, true, true, true]);
    expect(["de", "system", ""].map(isUiLang)).toEqual([false, false, false]);
  });

  it("lists English first, then the translations in registry order", () => {
    expect(uiLanguages().map((l) => l.code)).toEqual(["en", "fr", "pt-BR", "pt"]);
  });
});

describe("translating", () => {
  it("uses the selected language and falls back to English", () => {
    setUiLang("fr");
    expect(t("Save")).toBe("Enregistrer");
    expect(t("Not translated anywhere")).toBe("Not translated anywhere");
    expect(t("{n} days ago", { n: 3 })).toBe("il y a 3 jours");
  });

  it("treats an empty text as not translated yet", () => {
    setUiLang("fr");
    expect(t("Open")).toBe("Open");
  });

  it("keeps a marker the caller gave no value for", () => {
    expect(t("{n} days ago", {})).toBe("{n} days ago");
  });

  it("falls back to English for an unknown language", () => {
    setUiLang("klingon");
    expect(t("Save")).toBe("Save");
    expect(locale()).toBe("en-US");
  });

  it("gives dates the language's tag", () => {
    setUiLang("pt-BR");
    expect(locale()).toBe("pt-BR");
    setUiLang("pt");
    expect(locale()).toBe("pt-PT");
  });
});

describe("week labels", () => {
  const monday = new Date(2026, 9, 5);
  it("keeps the English and German forms", () => {
    expect(weekLabel(monday, "en-US")).toBe("October 5-11");
    expect(weekLabel(monday, "de-DE")).toBe("5.–11. Oktober");
  });

  it("lets the browser write the range for other languages", () => {
    const fr = weekLabel(monday, "fr-FR");
    expect(fr).toContain("octobre");
    expect(fr).toMatch(/5/);
    expect(fr).toMatch(/11/);
  });

  it("copes with a range across two months in another language", () => {
    const label = weekLabel(new Date(2026, 8, 28), "fr-FR");
    expect(label).toContain("septembre");
    expect(label).toContain("octobre");
  });
});

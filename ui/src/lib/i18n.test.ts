import { afterEach, describe, expect, it } from "vitest";
import { de } from "@/lib/de";
import { ago } from "@/lib/dates";
import { resolveUiLang, setUiLang, t } from "@/lib/i18n";

afterEach(() => setUiLang("en"));

describe("i18n", () => {
  it("follows the browser locale unless set", () => {
    expect(resolveUiLang("system", "de-AT")).toBe("de");
    expect(resolveUiLang("system", "fr-FR")).toBe("en");
    expect(resolveUiLang("en", "de-DE")).toBe("en");
    expect(resolveUiLang("de", "en-US")).toBe("de");
  });

  it("translates and fills in values, leaving unknown texts alone", () => {
    expect(t("Save")).toBe("Save");
    setUiLang("de");
    expect(t("Save")).toBe("Speichern");
    expect(t("Imported {title}", { title: "X" })).toBe("X importiert");
    expect(t("Not in the dictionary")).toBe("Not in the dictionary");
  });

  it("keeps the {placeholders} of every German text", () => {
    const markers = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join();
    for (const [en, ger] of Object.entries(de)) expect(markers(ger), en).toBe(markers(en));
  });

  it("translates relative dates", () => {
    setUiLang("de");
    expect(ago("2026-10-03T07:21:15+00:00", new Date("2026-10-05T07:21:15+00:00"))).toBe("vor 2 Tagen");
  });
});

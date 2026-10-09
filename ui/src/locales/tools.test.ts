import { describe, expect, it } from "vitest";
import { extractKeys, identifier, registerLocale, report, scaffold, validateNewLocale } from "@/locales/tools";
import type { Locale } from "@/locales";

describe("extractKeys", () => {
  it("finds the texts passed to t()", () => {
    const src = `
      <p>{t("Save")}</p>
      t("Imported {title}", { title })
      t(
        "On the next line",
      )
      t(\`a template\`)
      const x = t("with \\"quotes\\" and \\\\ a backslash");
    `;
    expect(extractKeys(src).sort()).toEqual(
      ["Save", "Imported {title}", "On the next line", "a template", 'with "quotes" and \\ a backslash'].sort(),
    );
  });

  it("ignores things that only look like calls", () => {
    expect(extractKeys('const at = format("x"); split("y"); fit("z"); t(variable); t(`a ${b}`)')).toEqual([]);
  });
});

describe("validateNewLocale", () => {
  const existing = ["de", "pt-BR"];
  it("accepts a new code, name and tag", () => {
    expect(validateNewLocale("fr", "Français", "fr-FR", existing)).toBeNull();
    expect(validateNewLocale("pt", "Português", "pt-PT", existing)).toBeNull();
    expect(validateNewLocale("zh-Hans", "简体中文", "zh-Hans-CN", existing)).toBeNull();
  });

  it("refuses a bad or taken code", () => {
    expect(validateNewLocale("French", "Français", "fr-FR", existing)).toContain("not a language code");
    expect(validateNewLocale("", "x", "fr-FR", existing)).toContain("not a language code");
    expect(validateNewLocale("../x", "x", "fr-FR", existing)).toContain("not a language code");
    expect(validateNewLocale("en", "English", "en-US", existing)).toContain("exists already");
    expect(validateNewLocale("DE", "Deutsch", "de-DE", existing)).toContain("not a language code");
    expect(validateNewLocale("pt-br", "x", "pt-BR", existing)).toContain("exists already");
  });

  it("refuses a missing name or a bad tag", () => {
    expect(validateNewLocale("fr", "  ", "fr-FR", existing)).toContain("needs a name");
    expect(validateNewLocale("fr", "Français", "not a tag!", existing)).toContain("not a language tag");
  });
});

describe("scaffold", () => {
  it("lists every text with an empty translation, escaped as TypeScript", () => {
    const file = scaffold("pt-BR", "Português (Brasil)", ["Save", 'Say "hi"', "{n} days ago", "Back\\slash", " from {device}"]);
    expect(file).toContain("export const ptBR: Record<string, string> = {");
    expect(file).toContain('  "Save": "",');
    expect(file).toContain('  "Say \\"hi\\"": "",');
    expect(file).toContain('  " from {device}": "",');
    // The file is valid TypeScript that evaluates to an object with those keys.
    const body = file.slice(file.indexOf("{\n") , file.lastIndexOf("};") + 1);
    const obj = new Function(`return ${body}`)() as Record<string, string>;
    expect(Object.keys(obj)).toEqual(["Save", 'Say "hi"', "{n} days ago", "Back\\slash", " from {device}"]);
    expect(Object.values(obj).every((v) => v === "")).toBe(true);
  });

  it("makes an identifier from a code", () => {
    expect(identifier("fr")).toBe("fr");
    expect(identifier("pt-BR")).toBe("ptBR");
    expect(identifier("zh-Hans-CN")).toBe("zhHansCN");
  });
});

describe("registerLocale", () => {
  const index = [
    'import { de } from "@/locales/de";',
    "",
    "export interface Locale {}",
    "",
    "export const locales: Locale[] = [",
    '  { code: "de", name: "Deutsch", tag: "de-DE", texts: de },',
    "];",
    "",
  ].join("\n");

  it("adds the import and the line", () => {
    const out = registerLocale(index, "fr", "Français", "fr-FR");
    expect(out).toContain('import { fr } from "@/locales/fr";');
    expect(out).toContain('  { code: "fr", name: "Français", tag: "fr-FR", texts: fr },');
    expect(out.indexOf('import { fr }')).toBeGreaterThan(out.indexOf('import { de }'));
    expect(out.indexOf('code: "fr"')).toBeGreaterThan(out.indexOf('code: "de"'));
    expect(out.trimEnd().endsWith("];")).toBe(true);
  });

  it("adds several languages in turn", () => {
    const out = registerLocale(registerLocale(index, "fr", "Français", "fr-FR"), "pt-BR", "Português", "pt-BR");
    expect(out.match(/^import /gm)).toHaveLength(3);
    expect(out.match(/code: "/g)).toHaveLength(3);
    expect(out).toContain("texts: ptBR },");
  });

  it("refuses a registry it does not recognise", () => {
    expect(() => registerLocale("export const locales = [];", "fr", "Français", "fr-FR")).toThrow(/expected shape/);
  });
});

describe("report", () => {
  const loc = (code: string, texts: Record<string, string>): Locale => ({ code, name: code, tag: `${code}-${code.toUpperCase()}`, texts });

  it("counts what is translated against every known text", () => {
    const [de, fr] = report([loc("de", { Save: "Speichern", Open: "Öffnen" }), loc("fr", { Save: "Enregistrer", Open: "" })], ["Save", "Close"]);
    expect(de.total).toBe(3);
    expect(de.translated).toBe(2);
    expect(de.missing).toEqual(["Close"]);
    expect(fr.translated).toBe(1);
    expect(fr.missing).toEqual(["Close", "Open"]);
    expect(de.errors).toEqual([]);
  });

  it("flags changed {markers}, but not an untranslated text", () => {
    const [r] = report([loc("fr", { "Imported {title}": "Importé {titre}", "Open {a}": "", "{n} days ago": "il y a {n} jours" })], []);
    expect(r.errors).toHaveLength(1);
    expect(r.errors[0]).toContain("Imported {title}");
  });

  it("flags a code used twice", () => {
    const rs = report([loc("fr", {}), loc("FR", {})], []);
    expect(rs.every((r) => r.errors.some((e) => e.includes("twice")))).toBe(true);
  });

  it("copes with no texts at all", () => {
    const [r] = report([loc("fr", {})], []);
    expect(r).toMatchObject({ total: 0, translated: 0, missing: [], errors: [] });
  });
});

// The logic behind `bun run i18n` and `bun run i18n:new` (scripts/i18n.ts), kept free of file access so it can be tested.
import type { Locale } from "@/locales";

/** English texts passed to `t("…")` in the source of one file. Texts used another way (a label prop, a list of names) are not found. */
export function extractKeys(source: string): string[] {
  const keys: string[] = [];
  for (const m of source.matchAll(/\bt\(\s*"((?:[^"\\\n]|\\.)*)"/g)) keys.push(JSON.parse(`"${m[1]}"`) as string);
  for (const m of source.matchAll(/\bt\(\s*`([^`$\\]*)`/g)) keys.push(m[1]);
  return keys;
}

/** The checks a language code and tag must pass before a file is made for them; `null` when they do. */
export function validateNewLocale(code: string, name: string, tag: string, existing: string[]): string | null {
  if (!/^[a-z]{2,3}(-[A-Za-z0-9]+)*$/.test(code)) return `"${code}" is not a language code such as fr or pt-BR`;
  if (code === "en" || existing.some((c) => c.toLowerCase() === code.toLowerCase())) return `${code} exists already`;
  if (name.trim() === "") return "the language needs a name, in that language";
  try {
    Intl.getCanonicalLocales(tag);
  } catch {
    return `"${tag}" is not a language tag such as fr-FR`;
  }
  return null;
}

/** `pt-BR` -> `ptBR`, a name for the exported object. */
export function identifier(code: string): string {
  return code.replace(/-(\w)/g, (_, c: string) => c.toUpperCase());
}

/** The file for a new language: every known text, not translated yet. */
export function scaffold(code: string, name: string, keys: string[]): string {
  const lines = keys.map((k) => `  ${JSON.stringify(k)}: "",`);
  return [
    `/** ${name} interface texts, keyed by the English text. An empty text stays English until it is translated. */`,
    `export const ${identifier(code)}: Record<string, string> = {`,
    ...lines,
    `};`,
    ``,
  ].join("\n");
}

/** The registry with a language added: its import and its line in the list. */
export function registerLocale(index: string, code: string, name: string, tag: string): string {
  const id = identifier(code);
  const lines = index.split("\n");
  const lastImport = lines.map((l) => l.startsWith('import { ') && l.includes('from "@/locales/')).lastIndexOf(true);
  const close = lines.lastIndexOf("];");
  if (lastImport < 0 || close < 0) throw new Error("locales/index.ts does not have the expected shape");
  lines.splice(close, 0, `  { code: ${JSON.stringify(code)}, name: ${JSON.stringify(name)}, tag: ${JSON.stringify(tag)}, texts: ${id} },`);
  lines.splice(lastImport + 1, 0, `import { ${id} } from "@/locales/${code}";`);
  return lines.join("\n");
}

export interface LocaleReport {
  code: string;
  name: string;
  /** Texts the app has (found in the source or in any language). */
  total: number;
  translated: number;
  /** Known texts this language has no translation for yet. */
  missing: string[];
  /** Things that must be fixed. */
  errors: string[];
}

const markers = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort().join();

/** Coverage and errors of every language. The known texts are the ones in the source plus the ones any language has. */
export function report(locales: Locale[], sourceKeys: string[]): LocaleReport[] {
  const known = [...new Set([...sourceKeys, ...locales.flatMap((l) => Object.keys(l.texts))])];
  return locales.map((l) => {
    const errors: string[] = [];
    if (locales.filter((o) => o.code.toLowerCase() === l.code.toLowerCase()).length > 1) errors.push("the code is used twice");
    for (const [en, text] of Object.entries(l.texts)) {
      if (text !== "" && markers(text) !== markers(en)) errors.push(`${JSON.stringify(en)}: the {markers} differ from the English text`);
    }
    const missing = known.filter((k) => !l.texts[k]);
    return { code: l.code, name: l.name, total: known.length, translated: known.length - missing.length, missing, errors };
  });
}

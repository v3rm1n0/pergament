import { de } from "@/lib/de";

/** Interface languages; the publication language is a separate setting. */
export type UiLang = "en" | "de";
export type UiLangSetting = "system" | UiLang;

const dictionaries: Record<UiLang, Record<string, string>> = { en: {}, de };

let current: UiLang = "en";

export function setUiLang(lang: UiLang) {
  current = lang;
}

export function uiLang(): UiLang {
  return current;
}

/** Browser locale for dates and numbers. */
export function locale(): string {
  return current === "de" ? "de-DE" : "en-US";
}

/** The interface language for a setting; "system" follows the browser locale. */
export function resolveUiLang(setting: UiLangSetting, browser: string): UiLang {
  if (setting !== "system") return setting;
  return browser.slice(0, 2).toLowerCase() === "de" ? "de" : "en";
}

/**
 * Translates an English text. Unknown texts stay English. `{name}` markers
 * are replaced from `vars`.
 */
export function t(text: string, vars?: Record<string, string | number>): string {
  const s = dictionaries[current][text] ?? text;
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

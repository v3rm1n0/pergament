import { locales } from "@/locales";

/** Interface language: `en` or the code of a language in `locales/index.ts`. The publication language is a separate setting. */
export type UiLang = string;
export type UiLangSetting = "system" | UiLang;

const byCode = new Map(locales.map((l) => [l.code.toLowerCase(), l]));

let current: UiLang = "en";

/** Interface languages in the order Settings lists them: English, then the translations. */
export function uiLanguages(): { code: UiLang; name: string }[] {
  return [{ code: "en", name: "English" }, ...locales.map((l) => ({ code: l.code, name: l.name }))];
}

export function isUiLang(code: string): boolean {
  return code === "en" || byCode.has(code.toLowerCase());
}

export function setUiLang(lang: UiLang) {
  current = isUiLang(lang) ? lang : "en";
}

export function uiLang(): UiLang {
  return current;
}

/** Browser locale for dates and numbers. */
export function locale(): string {
  return byCode.get(current.toLowerCase())?.tag ?? "en-US";
}

/**
 * The interface language for a setting; "system" follows the browser locale:
 * an exact match (`pt-BR`) first, then the language alone (`pt`), else English.
 */
export function resolveUiLang(setting: UiLangSetting, browser: string): UiLang {
  if (setting !== "system") return isUiLang(setting) ? setting : "en";
  const tag = browser.toLowerCase();
  return byCode.get(tag)?.code ?? byCode.get(tag.split("-")[0])?.code ?? "en";
}

/**
 * Translates an English text. Unknown or untranslated (empty) texts stay
 * English. `{name}` markers are replaced from `vars`.
 */
export function t(text: string, vars?: Record<string, string | number>): string {
  const s = byCode.get(current.toLowerCase())?.texts[text] || text;
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

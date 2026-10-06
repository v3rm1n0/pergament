import type { UiLangSetting } from "@/lib/i18n";

export type Theme = "system" | "light" | "dark";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage unavailable: settings just do not persist.
  }
}

export function loadTheme(): Theme {
  const t = read("theme");
  return t === "light" || t === "dark" ? t : "system";
}

export function saveTheme(theme: Theme) {
  write("theme", theme);
}

export function prefersDark(theme: Theme, systemDark: boolean): boolean {
  return theme === "dark" || (theme === "system" && systemDark);
}

export function loadFontScale(): number {
  const n = Number(read("fontScale"));
  return n >= 0.8 && n <= 1.6 ? n : 1;
}

export function saveFontScale(scale: number) {
  write("fontScale", String(scale));
}

/** Catalog language code for a browser locale (codes verified in docs/FORMAT.md). */
export function defaultLangCode(locale: string): string {
  const map: Record<string, string> = { de: "X", en: "E", es: "S", fr: "F", it: "I" };
  return map[locale.slice(0, 2).toLowerCase()] ?? "E";
}

export function loadLang(): string {
  const l = read("lang");
  return l && /^[A-Z0-9-]{1,8}$/.test(l) ? l : defaultLangCode(navigator.language);
}

export function saveLang(code: string) {
  write("lang", code);
}

export function loadUiLang(): UiLangSetting {
  const l = read("uiLang");
  return l === "en" || l === "de" ? l : "system";
}

export function saveUiLang(lang: UiLangSetting) {
  write("uiLang", lang);
}

/** Publications shown for a language; entries without a recorded code always show. */
export function inLanguage<T extends { langCode: string | null }>(items: T[], code: string): T[] {
  return items.filter((p) => p.langCode == null || p.langCode === code);
}

/** Library directories marked as favorites. */
export function loadFavorites(): string[] {
  try {
    const v: unknown = JSON.parse(read("favorites") ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

export function toggleFavorite(dir: string): string[] {
  const list = loadFavorites();
  const next = list.includes(dir) ? list.filter((d) => d !== dir) : [...list, dir];
  write("favorites", JSON.stringify(next));
  return next;
}

/** Highlight color used for a note on a new selection. */
export function loadLastColor(): number {
  const n = Number(read("highlightColor"));
  return n >= 1 && n <= 6 ? n : 1;
}

export function saveLastColor(color: number) {
  write("highlightColor", String(color));
}

import { isUiLang, type UiLangSetting } from "@/lib/i18n";

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

/** Storage keys of the theme and the interface language, which other windows follow. */
export const LOOK_KEYS = ["theme", "uiLang"];

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

/** The font of the app: the system one, or a typeface designed for legibility. */
export type ReadingFont = "standard" | "legible";

export function loadReadingFont(): ReadingFont {
  return read("readingFont") === "legible" ? "legible" : "standard";
}

export function saveReadingFont(font: ReadingFont) {
  write("readingFont", font);
}

/** Whether a backup is merged into the data instead of replacing it. */
export function loadMergeBackups(): boolean {
  return read("mergeBackups") === "true";
}

export function saveMergeBackups(on: boolean) {
  write("mergeBackups", String(on));
}

/** Content width: a centered column (the default) or the whole window. */
export type Layout = "centered" | "wide";

export function loadLayout(): Layout {
  return read("layout") === "wide" ? "wide" : "centered";
}

export function saveLayout(layout: Layout) {
  write("layout", layout);
}

/**
 * Catalog language code for a browser locale (codes verified in docs/FORMAT.md and the jw.org language list, where
 * `T` is Portuguese (Brazil) and `TPO` Portuguese (Portugal)).
 */
export function defaultLangCode(locale: string): string {
  const map: Record<string, string> = { de: "X", en: "E", es: "S", fr: "F", it: "I", pt: "T", "pt-pt": "TPO" };
  const tag = locale.toLowerCase();
  return map[tag] ?? map[tag.slice(0, 2)] ?? "E";
}

export function loadLang(): string {
  const l = read("lang");
  return l && /^[A-Z0-9-]{1,8}$/.test(l) ? l : defaultLangCode(navigator.language);
}

export function saveLang(code: string) {
  write("lang", code);
}

/** Catalog language code the Teaching Toolbox shows; `null` when no override is stored. */
export function loadToolboxLang(): string | null {
  const l = read("toolboxLang");
  return l && /^[A-Z0-9-]{1,8}$/.test(l) ? l : null;
}

/** Stores the Teaching Toolbox language code, or clears the override with `null`. */
export function saveToolboxLang(code: string | null) {
  try {
    if (code) localStorage.setItem("toolboxLang", code);
    else localStorage.removeItem("toolboxLang");
  } catch {
    // Storage unavailable: the setting just does not persist.
  }
}

/** Languages the What's New list shows; `null` is the default (the app language). */
export function loadWhatsNewLangs(): string[] | null {
  try {
    const v: unknown = JSON.parse(read("whatsNewLangs") ?? "null");
    if (!Array.isArray(v)) return null;
    const codes = [...new Set(v.filter((x): x is string => typeof x === "string" && /^[A-Z0-9-]{1,8}$/.test(x)))];
    return codes.length > 0 ? codes : null;
  } catch {
    return null;
  }
}

export function saveWhatsNewLangs(codes: string[]) {
  write("whatsNewLangs", JSON.stringify(codes));
}

export function loadUiLang(): UiLangSetting {
  const l = read("uiLang");
  return l && isUiLang(l) ? l : "system";
}

export function saveUiLang(lang: UiLangSetting) {
  write("uiLang", lang);
}

/** Publications shown for a language; entries without a recorded code always show. */
export function inLanguage<T extends { langCode: string | null }>(items: T[], code: string): T[] {
  return items.filter((p) => p.langCode == null || p.langCode === code);
}

/** Highlight color used for a note on a new selection. */
export function loadLastColor(): number {
  const n = Number(read("highlightColor"));
  return n >= 1 && n <= 6 ? n : 1;
}

export function saveLastColor(color: number) {
  write("highlightColor", String(color));
}

/** Saved text of an answer field in a publication (`div.gen-field`). */
export function loadAnswer(key: string): string {
  return read(`answer:${key}`) ?? "";
}

export function saveAnswer(key: string, text: string) {
  try {
    if (text) localStorage.setItem(`answer:${key}`, text);
    else localStorage.removeItem(`answer:${key}`);
  } catch {
    // Storage unavailable: the answer is not kept.
  }
}

/** Library directories of the Bibles shown as parallel translations, in order; `null` is the default (all). */
export function loadParallel(): string[] | null {
  try {
    const v: unknown = JSON.parse(read("parallelBibles") ?? "null");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : null;
  } catch {
    return null;
  }
}

export function saveParallel(dirs: string[]) {
  write("parallelBibles", JSON.stringify(dirs));
}

/** Highest video height (in pixels) picked by default; larger files stream slowly. */
export function loadMediaQuality(): number {
  const n = Number(read("mediaQuality"));
  return n >= 144 && n <= 2160 ? n : 480;
}

export function saveMediaQuality(height: number) {
  write("mediaQuality", String(height));
}

/** Quality is automatic unless the user picked a fixed one. */
export function loadAutoQuality(): boolean {
  const v = read("mediaQuality");
  return v === null || v === "auto";
}

export function saveAutoQuality() {
  write("mediaQuality", "auto");
}

/** Whether the second display window is open. */
export function loadSecondDisplay(): boolean {
  return read("secondDisplay") === "1";
}

export function saveSecondDisplay(on: boolean) {
  write("secondDisplay", on ? "1" : "0");
}

/** Whether the catalog is checked for updates to downloaded publications at startup. */
export function loadUpdateCheck(): boolean {
  return read("updateCheck") !== "0";
}

export function saveUpdateCheck(on: boolean) {
  write("updateCheck", on ? "1" : "0");
}

/** The Bible (publication symbol) last picked per language code. */
export function loadBibles(): Record<string, string> {
  try {
    const v: unknown = JSON.parse(read("bibles") ?? "{}");
    if (!v || typeof v !== "object" || Array.isArray(v)) return {};
    return Object.fromEntries(Object.entries(v).filter((e): e is [string, string] => typeof e[1] === "string"));
  } catch {
    return {};
  }
}

export function saveBibles(bibles: Record<string, string>) {
  write("bibles", JSON.stringify(bibles));
}

/** The size the user last gave the note window, in logical pixels. */
export function loadNoteWindowSize(): { width: number; height: number } | null {
  try {
    const s = JSON.parse(read("noteWindowSize") ?? "null");
    return Number.isFinite(s?.width) && Number.isFinite(s?.height) ? { width: s.width, height: s.height } : null;
  } catch {
    return null;
  }
}

export function saveNoteWindowSize(size: { width: number; height: number }) {
  write("noteWindowSize", JSON.stringify(size));
}

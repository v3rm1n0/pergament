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

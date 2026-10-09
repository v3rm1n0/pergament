/** Local calendar date as `YYYY-MM-DD`. */
export function isoDate(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** Monday of the week containing `d` (meeting weeks run Monday–Sunday). */
export function weekStart(d: Date): Date {
  const out = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const offset = (out.getDay() + 6) % 7; // Monday = 0
  out.setDate(out.getDate() - offset);
  return out;
}

export function addDays(d: Date, days: number): Date {
  const out = new Date(d);
  out.setDate(out.getDate() + days);
  return out;
}

import { locale as uiLocale, t } from "@/lib/i18n";

/** "October 5-11", or "September 28 - October 4" across months. */
export function weekLabel(monday: Date, locale = uiLocale()): string {
  const sunday = addDays(monday, 6);
  const month = (d: Date) => d.toLocaleDateString(locale, { month: "long" });
  if (locale.startsWith("de")) {
    // "5.–11. Oktober" or "28. September – 4. Oktober"
    return monday.getMonth() === sunday.getMonth()
      ? `${monday.getDate()}.–${sunday.getDate()}. ${month(monday)}`
      : `${monday.getDate()}. ${month(monday)} – ${sunday.getDate()}. ${month(sunday)}`;
  }
  if (locale.startsWith("en")) {
    return monday.getMonth() === sunday.getMonth()
      ? `${month(monday)} ${monday.getDate()}-${sunday.getDate()}`
      : `${month(monday)} ${monday.getDate()} - ${month(sunday)} ${sunday.getDate()}`;
  }
  // Any other language: the browser knows how it writes a range of days.
  return new Intl.DateTimeFormat(locale, { month: "long", day: "numeric" }).formatRange(monday, sunday);
}

/** "1 day ago", "3 weeks ago", ... for an RFC 3339 timestamp. */
export function ago(timestamp: string | null, now = new Date()): string {
  if (!timestamp) return "";
  const days = Math.floor((now.getTime() - new Date(timestamp).getTime()) / 86_400_000);
  if (Number.isNaN(days)) return "";
  if (days <= 0) return t("today");
  if (days < 14) return days === 1 ? t("1 day ago") : t("{n} days ago", { n: days });
  if (days < 60) return t("{n} weeks ago", { n: Math.floor(days / 7) });
  return t("{n} months ago", { n: Math.floor(days / 30) });
}

/** "Study Articles for October 5 to November 1" style range. */
export function rangeLabel(start: string, end: string, locale = uiLocale()): string {
  const f = (s: string) =>
    new Date(`${s}T12:00:00`).toLocaleDateString(locale, { month: "long", day: "numeric" });
  return t("{start} to {end}", { start: f(start), end: f(end) });
}

/** `20261007` -> local date. */
export function fromDateNumber(n: number): Date {
  return new Date(Math.floor(n / 10000), (Math.floor(n / 100) % 100) - 1, n % 100);
}

/** "Wednesday, October 7". */
export function longDate(d: Date, locale = uiLocale()): string {
  return d.toLocaleDateString(locale, { weekday: "long", month: "long", day: "numeric" });
}

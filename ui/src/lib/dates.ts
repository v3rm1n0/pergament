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

/** "October 5-11", or "September 28 - October 4" across months. */
export function weekLabel(monday: Date, locale = "en-US"): string {
  const sunday = addDays(monday, 6);
  const month = (d: Date) => d.toLocaleDateString(locale, { month: "long" });
  return monday.getMonth() === sunday.getMonth()
    ? `${month(monday)} ${monday.getDate()}-${sunday.getDate()}`
    : `${month(monday)} ${monday.getDate()} - ${month(sunday)} ${sunday.getDate()}`;
}

/** "1 day ago", "3 weeks ago", ... for an RFC 3339 timestamp. */
export function ago(timestamp: string | null, now = new Date()): string {
  if (!timestamp) return "";
  const days = Math.floor((now.getTime() - new Date(timestamp).getTime()) / 86_400_000);
  if (Number.isNaN(days)) return "";
  if (days <= 0) return "today";
  if (days < 14) return days === 1 ? "1 day ago" : `${days} days ago`;
  if (days < 60) return `${Math.floor(days / 7)} weeks ago`;
  return `${Math.floor(days / 30)} months ago`;
}

/** "Study Articles for October 5 to November 1" style range. */
export function rangeLabel(start: string, end: string, locale = "en-US"): string {
  const f = (s: string) =>
    new Date(`${s}T12:00:00`).toLocaleDateString(locale, { month: "long", day: "numeric" });
  return `${f(start)} to ${f(end)}`;
}

/** Automatic quality for recordings that exist as separate files per quality. */

interface Labeled {
  label: string;
  url: string;
}

const height = (s: Labeled) => parseInt(s.label, 10) || 0;
const ascending = <T extends Labeled>(sources: T[]) => [...sources].sort((a, b) => height(a) - height(b));

/** Auto never goes above this (the largest quality the catalog has). */
export const AUTO_CEILING = 1080;
/** Smallest height worth starting at when the window allows more. */
const AUTO_FLOOR = 360;

/**
 * The quality to start with: the largest that fits the window (in device
 * pixels), but at least `AUTO_FLOOR` and at most `AUTO_CEILING`.
 */
export function autoStart<T extends Labeled>(sources: T[], windowHeight: number): T | undefined {
  const sorted = ascending(sources);
  const target = Math.min(AUTO_CEILING, Math.max(AUTO_FLOOR, windowHeight));
  return [...sorted].reverse().find((s) => height(s) <= target) ?? sorted[0];
}

/** The next smaller quality, if there is one. */
export function stepDown<T extends Labeled>(sources: T[], current: T): T | undefined {
  const sorted = ascending(sources);
  const i = sorted.findIndex((s) => s.url === current.url);
  return i > 0 ? sorted[i - 1] : undefined;
}

/**
 * The next larger quality, if it is within the ceiling and has not stalled
 * before (`blocked` holds the URLs that did).
 */
export function stepUp<T extends Labeled>(
  sources: T[],
  current: T,
  ceiling: number,
  blocked: ReadonlySet<string>,
): T | undefined {
  const sorted = ascending(sources);
  const i = sorted.findIndex((s) => s.url === current.url);
  const next = sorted[i + 1];
  return i >= 0 && next && height(next) <= ceiling && !blocked.has(next.url) ? next : undefined;
}

/** Stalls within this window mean the connection cannot keep up. */
export const STALL_WINDOW_MS = 30_000;
/** Seconds of media buffered ahead that count as healthy, and for how long. */
export const HEALTHY_AHEAD = 30;
export const HEALTHY_FOR_MS = 20_000;

/** Whether the stalls so far call for a smaller quality. */
export function tooManyStalls(stalls: readonly number[], now: number): boolean {
  return stalls.filter((t) => now - t <= STALL_WINDOW_MS).length >= 2;
}

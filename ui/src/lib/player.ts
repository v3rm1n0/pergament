/** One quality of a recording the player can play. */
export interface PlayerSource {
  /** `480p`, empty for audio. */
  label: string;
  url: string;
  size: number | null;
  subtitles: string | null;
}

/** What the player shows: a recording and the qualities available for it. */
export interface PlayerItem {
  title: string;
  kind: "audio" | "video";
  poster: string | null;
  sources: PlayerSource[];
}

export const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

const height = (s: { label: string }) => parseInt(s.label, 10) || 0;

/** The best source up to `maxHeight`, or the smallest one if all are larger. */
export function pickSource<T extends { label: string }>(sources: T[], maxHeight: number): T | undefined {
  const sorted = [...sources].sort((a, b) => height(a) - height(b));
  return [...sorted].reverse().find((s) => height(s) <= maxHeight) ?? sorted[0];
}

/** `0:05`, `5:27`, `1:02:03`. */
export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

/** Size in MB for menus, at least 1. */
export const sizeMb = (bytes: number) => `${Math.max(1, Math.round(bytes / 1e6))} MB`;

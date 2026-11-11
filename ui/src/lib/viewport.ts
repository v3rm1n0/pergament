import { matches, useMediaQuery } from "./media-query";

/** Phone-sized windows: the icon rail becomes a bottom tab bar and side panes become sheets. */
export const COMPACT = "(max-width: 719px)";

/** Whether the window is narrow enough for the compact layout, read once (for initial state). */
export function isCompact(): boolean {
  return matches(COMPACT);
}

/** Whether the window is narrow enough for the compact layout; follows resizes while mounted. */
export function useCompact(): boolean {
  return useMediaQuery(COMPACT);
}

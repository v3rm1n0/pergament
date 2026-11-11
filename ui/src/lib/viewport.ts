import { useEffect, useState } from "react";

/** Phone-sized windows: the icon rail becomes a bottom tab bar and side panes become sheets. */
const COMPACT = "(max-width: 719px)";

/** Whether the window is narrow enough for the compact layout, read once (for initial state). */
export function isCompact(): boolean {
  return window.matchMedia?.(COMPACT).matches ?? false;
}

/** Whether the window is narrow enough for the compact layout; follows resizes while mounted. */
export function useCompact(): boolean {
  const [compact, setCompact] = useState(isCompact);
  useEffect(() => {
    const mq = window.matchMedia?.(COMPACT);
    if (!mq) return;
    const on = (e: MediaQueryListEvent) => setCompact(e.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return compact;
}
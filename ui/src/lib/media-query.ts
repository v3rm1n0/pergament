import { useEffect, useState } from "react";

/** Whether a media query matches right now; false when the environment has no matchMedia. */
export function matches(query: string): boolean {
  return window.matchMedia?.(query).matches ?? false;
}

/** Whether a media query matches; follows changes while mounted. */
export function useMediaQuery(query: string): boolean {
  const [match, setMatch] = useState(() => matches(query));
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const on = (e: MediaQueryListEvent) => setMatch(e.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, [query]);
  return match;
}

import { useMediaQuery } from "./media-query";

/** Whether the system asks for a dark appearance; follows changes while mounted. */
export function useSystemDark(): boolean {
  return useMediaQuery("(prefers-color-scheme: dark)");
}

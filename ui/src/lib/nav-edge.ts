import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isIOS } from "./platform";

/** A side edge of the window, or null for none. */
export type NavEdge = "left" | "right" | null;

const edge = (value: unknown): NavEdge => (value === "left" || value === "right" ? value : null);

/**
 * The edge the iOS shell wants the navigation along (WebShell.swift): where the system puts its own vertical bar on
 * a folding iPhone, or the leading edge of a wide window. Null elsewhere, and until the shell has answered.
 */
export function useNavEdge(): NavEdge {
  const [value, setValue] = useState<NavEdge>(null);
  useEffect(() => {
    if (!isIOS) return;
    let mounted = true;
    const set = (v: unknown) => {
      if (mounted) setValue(edge(v));
    };
    // Listening before asking, so a change while the question is on its way is not missed.
    const changed = listen("nav-edge", (e) => set(e.payload));
    invoke("nav_edge").then(set, () => {});
    return () => {
      mounted = false;
      void changed.then((off) => off());
    };
  }, []);
  return value;
}

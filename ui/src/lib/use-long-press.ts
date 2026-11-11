import { useEffect, useLayoutEffect, useMemo, useRef } from "react";

export interface PressPoint {
  clientX: number;
  clientY: number;
}

/** The pointer events a long press listens to; they work as React props and as native listeners. */
export interface PressHandlers {
  onPointerDown: (e: PointerEvent | React.PointerEvent) => void;
  onPointerMove: (e: PointerEvent | React.PointerEvent) => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
  /** Swallows the click that follows a press that fired, so the tile under the finger does not open too. */
  onClickCapture: (e: MouseEvent | React.MouseEvent) => void;
}

/** How far a finger may drift and still be holding. */
const SLOP = 10;

/**
 * Calls `handler` once a touch or pen is held for `ms` without moving 10 px or scrolling. A mouse is left alone, so
 * desktop clicks and drags behave as before.
 */
export function useLongPress(handler: (e: PressPoint) => void, ms = 500): PressHandlers {
  const latest = useRef(handler);
  useLayoutEffect(() => {
    latest.current = handler;
  });
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const origin = useRef<PressPoint | null>(null);
  const fired = useRef(false);
  const settle = useRef<ReturnType<typeof setTimeout>>(undefined);

  const cancel = () => {
    clearTimeout(timer.current);
    origin.current = null;
    window.removeEventListener("scroll", cancel, true);
  };
  useEffect(
    () => () => {
      cancel();
      clearTimeout(settle.current);
    },
    [],
  );

  /** The click of a fired press follows its release at once; if none comes, the next click is a real one. */
  const release = () => {
    cancel();
    if (!fired.current) return;
    clearTimeout(settle.current);
    settle.current = setTimeout(() => (fired.current = false), 50);
  };

  return useMemo(
    () => ({
      onPointerDown: (e) => {
        clearTimeout(settle.current);
        fired.current = false;
        if (e.pointerType === "mouse") return;
        cancel();
        origin.current = { clientX: e.clientX, clientY: e.clientY };
        const point = origin.current;
        window.addEventListener("scroll", cancel, true);
        timer.current = setTimeout(() => {
          cancel();
          fired.current = true;
          latest.current(point);
        }, ms);
      },
      onPointerMove: (e) => {
        const o = origin.current;
        if (o && Math.hypot(e.clientX - o.clientX, e.clientY - o.clientY) >= SLOP) cancel();
      },
      onPointerUp: release,
      onPointerCancel: release,
      onClickCapture: (e) => {
        if (!fired.current) return;
        fired.current = false;
        e.preventDefault();
        e.stopPropagation();
      },
    }),
    [ms],
  );
}

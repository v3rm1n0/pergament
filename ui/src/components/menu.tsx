import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";

const MARGIN = 8;

/** Popover next to `anchor`, kept inside the window: it opens upwards unless there is no room and lines up with the anchor's right edge unless that would push it out. */
export function Menu({ anchor, onClose, children }: { anchor: DOMRect; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  // Placed after measuring, so the clamping knows the real size.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const left = Math.max(MARGIN, Math.min(anchor.right - width, window.innerWidth - width - MARGIN));
    const up = anchor.top - height - 4 >= MARGIN;
    const top = up ? anchor.top - height - 4 : Math.max(MARGIN, Math.min(anchor.bottom + 4, window.innerHeight - height - MARGIN));
    setPos({ left, top });
  }, [anchor]);

  // Fixed, so rows that scroll sideways do not clip it.
  const style: CSSProperties = pos ?? { left: 0, top: 0, visibility: "hidden" };
  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        ref={ref}
        role="menu"
        style={style}
        className="fixed z-50 flex min-w-[200px] max-w-[calc(100vw-16px)] flex-col divide-y divide-line rounded-md border border-line bg-surface shadow-sm"
      >
        {children}
      </div>
    </>
  );
}

export const menuItem = "px-4 py-2 text-left text-[0.85rem] hover:bg-bar";

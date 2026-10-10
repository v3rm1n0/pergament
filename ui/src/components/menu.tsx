import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";

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

  // Keyboard: focus moves into the menu when it opens, the arrow keys, Home and End move between its items, and
  // focus goes back to where it was when it closes.
  const before = useRef<HTMLElement | null>(null);
  useEffect(() => {
    before.current = document.activeElement as HTMLElement | null;
    return () => {
      // Back to where it was, unless the choice moved focus on, say into a dialog it opened.
      const now = document.activeElement;
      if (!now || now === document.body || ref.current?.contains(now)) before.current?.focus?.({ preventScroll: true });
    };
  }, []);
  // The menu is hidden until it has been measured, and a hidden item cannot take focus, so this waits for that.
  const placed = pos !== null;
  useEffect(() => {
    if (placed) ref.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus({ preventScroll: true });
  }, [placed]);
  const onKeyDown = (e: ReactKeyboardEvent) => {
    const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
    if (!keys.includes(e.key)) return;
    const items = [...(ref.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    if (items.length === 0) return;
    e.preventDefault();
    const at = items.indexOf(document.activeElement as HTMLElement);
    const next =
      e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : (at + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    items[next].focus();
  };

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
        onKeyDown={onKeyDown}
        style={style}
        className="fixed z-50 flex min-w-[200px] max-w-[calc(100vw-16px)] flex-col divide-y divide-line rounded-md border border-line bg-surface shadow-sm"
      >
        {children}
      </div>
    </>
  );
}

export const menuItem = "px-4 py-2 text-left text-[0.85rem] hover:bg-bar";

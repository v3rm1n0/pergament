import { useEffect } from "react";
import type { CSSProperties, ReactNode } from "react";

export function Menu({ anchor, onClose, children }: { anchor: DOMRect; onClose: () => void; children: ReactNode }) {
  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);
  // Fixed, so rows that scroll sideways do not clip it; it opens upwards unless there is no room.
  const up = anchor.top > 170;
  const style: CSSProperties = {
    right: Math.max(8, window.innerWidth - anchor.right),
    ...(up ? { bottom: window.innerHeight - anchor.top + 4 } : { top: anchor.bottom + 4 }),
  };
  return (
    <>
      <div className="fixed inset-0 z-40" onClick={onClose} />
      <div
        role="menu"
        style={style}
        className="fixed z-50 flex min-w-[200px] flex-col bg-surface py-1 shadow-xl ring-1 ring-line"
      >
        {children}
      </div>
    </>
  );
}

export const menuItem = "px-4 py-2 text-left text-[0.85rem] hover:bg-bar";

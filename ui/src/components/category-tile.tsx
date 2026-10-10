import type { LucideIcon } from "lucide-react";

/** A category as a bordered row: an icon in a muted square, then its name. */
export function CategoryTile({ icon: Icon, name, onOpen }: { icon: LucideIcon; name: string; onOpen: () => void }) {
  return (
    <button
      onClick={onOpen}
      className="flex h-16 items-center gap-3 rounded-md border border-line px-3 text-left hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span className="flex size-10 shrink-0 items-center justify-center rounded-md bg-bar text-muted">
        <Icon size={22} strokeWidth={1.4} />
      </span>
      <span className="line-clamp-2 min-w-0 text-sm leading-snug">{name}</span>
    </button>
  );
}

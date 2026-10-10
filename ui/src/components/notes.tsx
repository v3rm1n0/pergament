import { useEffect, useRef, useState } from "react";
import { NotebookPen, Trash2, X } from "lucide-react";
import type { Note, NoteInput } from "@/lib/api";
import { HIGHLIGHT_COLORS } from "@/lib/marks";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

/** Color swatches plus note and delete actions, shown at a selection or highlight. */
export function MarkToolbar({
  x,
  y,
  current,
  onColor,
  onNote,
  onDelete,
  onClose,
}: {
  x: number;
  y: number;
  current?: number;
  onColor: (color: number) => void;
  onNote: () => void;
  onDelete?: () => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", esc);
    };
  }, [onClose]);
  return (
    <div
      ref={ref}
      style={{ left: x, top: y }}
      className="fixed z-50 flex -translate-x-1/2 -translate-y-full items-center gap-1.5 rounded-md border border-line bg-surface px-2 py-1.5 shadow-sm"
    >
      {HIGHLIGHT_COLORS.map((c) => (
        <button
          key={c.index}
          title={t(c.name)}
          aria-label={t("Highlight {color}", { color: t(c.name) })}
          onClick={() => onColor(c.index)}
          className={cn(
            `hl-dot-${c.index} h-6 w-6 rounded-full ring-1 ring-black/20`,
            current === c.index && "ring-2 ring-fg",
          )}
        />
      ))}
      <span className="mx-1 h-5 w-px bg-line" />
      <button title={t("Note")} aria-label={t("Add note")} onClick={onNote} className="p-1 hover:text-accent">
        <NotebookPen size={18} />
      </button>
      {onDelete && (
        <button title={t("Remove highlight")} aria-label={t("Remove highlight")} onClick={onDelete} className="p-1 hover:text-accent">
          <Trash2 size={18} />
        </button>
      )}
    </div>
  );
}

/** A note as shown in the study pane and in Personal Study. */
export function NoteCard({ note, onClick, subtitle }: { note: Note; onClick?: () => void; subtitle?: string }) {
  return (
    <button onClick={onClick} className="flex w-full gap-3 rounded-md border border-line p-3 text-left hover:bg-bar">
      <span className={cn("w-1 shrink-0 self-stretch", note.color ? `hl-dot-${note.color}` : "bg-line")} />
      <div className="min-w-0 flex-1">
        {subtitle && <div className="truncate text-xs text-muted">{subtitle}</div>}
        {note.title && <div className="font-semibold">{note.title}</div>}
        {note.content && <div className="line-clamp-4 whitespace-pre-wrap text-sm text-fg/85">{note.content}</div>}
        {note.tags.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1">
            {note.tags.map((t) => (
              <span key={t} className="rounded border border-line px-1.5 py-0.5 text-xs">
                {t}
              </span>
            ))}
          </div>
        )}
      </div>
    </button>
  );
}

/** Dialog to write or change a note. */
export function NoteEditor({
  note,
  onSave,
  onDelete,
  onClose,
}: {
  note: Partial<Note>;
  onSave: (input: Omit<NoteInput, "guid" | "blockType" | "blockIdentifier" | "markGuid">) => void;
  onDelete?: () => void;
  onClose: () => void;
}) {
  const [title, setTitle] = useState(note.title ?? "");
  const [content, setContent] = useState(note.content ?? "");
  const [tags, setTags] = useState((note.tags ?? []).join(", "));
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label={note.guid ? t("Edit note") : t("New note")}
        onMouseDown={(e) => e.stopPropagation()}
        className="flex w-[min(90vw,560px)] flex-col gap-3 rounded-md border border-line bg-surface p-5 shadow-sm"
      >
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">{note.guid ? t("Edit note") : t("New note")}</h2>
          <button aria-label={t("Close")} onClick={onClose}>
            <X size={20} />
          </button>
        </div>
        <input
          id="note-title"
          autoFocus
          placeholder={t("Title")}
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          className="rounded-md border border-line bg-surface px-3 py-2 focus:border-accent"
        />
        <textarea
          id="note-content"
          placeholder={t("Note")}
          rows={8}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          className="resize-y rounded-md border border-line bg-surface px-3 py-2 focus:border-accent"
        />
        <input
          id="note-tags"
          placeholder={t("Tags, separated by commas")}
          value={tags}
          onChange={(e) => setTags(e.target.value)}
          className="rounded-md border border-line bg-surface px-3 py-2 text-sm focus:border-accent"
        />
        <div className="flex items-center gap-2">
          {onDelete && (
            <button onClick={onDelete} className="flex items-center gap-1.5 px-3 py-2 text-sm text-muted hover:text-fg">
              <Trash2 size={16} /> {t("Delete")}
            </button>
          )}
          <span className="flex-1" />
          <button onClick={onClose} className="px-4 py-2 text-sm hover:bg-bar">
            {t("Cancel")}
          </button>
          <button
            disabled={!title.trim() && !content.trim()}
            onClick={() =>
              onSave({
                title: title.trim(),
                content,
                tags: tags
                  .split(",")
                  .map((t) => t.trim())
                  .filter(Boolean),
              })
            }
            className="rounded-md bg-brand px-4 py-2 text-sm text-brand-fg hover:opacity-90 dark:bg-fg dark:text-surface disabled:opacity-50"
          >
            {t("Save")}
          </button>
        </div>
      </div>
    </div>
  );
}

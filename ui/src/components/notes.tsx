import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp, NotebookPen, SquareArrowOutUpRight, Trash2 } from "lucide-react";
import { useApp } from "@/app";
import { api, type Note } from "@/lib/api";
import { fieldsOf, parseTags, sameFields, type NoteDraft, type NoteFields } from "@/lib/notes";
import { renderMarkdown } from "@/lib/markdown";
import { linkEdit, useLinkBible } from "@/lib/note-links";
import type { Book } from "@/lib/bible";
import { HIGHLIGHT_COLORS } from "@/lib/marks";
import { canOpenNoteWindow } from "@/lib/platform";
import { sendNoteLink } from "@/lib/note-window";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
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

/**
 * Opens a link clicked in a note: the app's own links, such as a Bible verse, with `onAppLink`, else in the main
 * window; others in the browser.
 */
function openNoteLink(a: HTMLAnchorElement, onAppLink: ((href: string) => void) | undefined, onError: (message: string) => void) {
  const href = a.getAttribute("href") ?? "";
  if (href.startsWith("pergament:")) (onAppLink ?? sendNoteLink)(href);
  else api.openExternal(a.href).catch((e) => onError(String(e)));
}

/**
 * A note as shown in the study pane and in Personal Study. With `onToggle`, a
 * note cut off at four lines can be opened to its whole text, links and all.
 */
export function NoteCard({
  note,
  onClick,
  subtitle,
  expanded = false,
  onToggle,
  onAppLink,
}: {
  note: Note;
  onClick?: () => void;
  subtitle?: string;
  expanded?: boolean;
  onToggle?: () => void;
  /** Opens the app's own links, such as a Bible verse; without it the main window opens them. */
  onAppLink?: (href: string) => void;
}) {
  const body = useRef<HTMLDivElement>(null);
  const [clipped, setClipped] = useState(false);
  // Measured while clamped: the toggle only shows where there is more to see. A pane that gets narrower or wider
  // wraps the text anew, so it is measured again then.
  useLayoutEffect(() => {
    const el = body.current;
    if (!el || expanded) return;
    const measure = () => setClipped(el.scrollHeight > el.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [note.content, expanded]);
  const toggle = onToggle && (clipped || expanded);
  /** Links in the whole note open in the app or the browser; a click elsewhere on it edits the note. */
  const onBodyClick = (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) {
      // Selecting text to copy it is not a request to edit.
      if (window.getSelection()?.isCollapsed !== false) onClick?.();
      return;
    }
    e.preventDefault();
    // The study pane would otherwise follow the link itself.
    e.stopPropagation();
    openNoteLink(a, onAppLink, () => undefined);
  };
  const bar = <span className={cn("w-1 shrink-0 self-stretch", note.color ? `hl-dot-${note.color}` : "bg-line")} />;
  const tags = note.tags.length > 0 && (
    <div className="mt-1.5 flex flex-wrap gap-1">
      {note.tags.map((t) => (
        <span key={t} className="rounded border border-line px-1.5 py-0.5 text-xs">
          {t}
        </span>
      ))}
    </div>
  );
  const head = (
    <>
      {subtitle && <div className="truncate text-xs text-muted">{subtitle}</div>}
      {note.title && <div className="font-semibold">{note.title}</div>}
    </>
  );
  const frame = cn("flex w-full gap-3 rounded-md border border-line p-3 text-left hover:bg-bar", toggle && "pr-9");
  const card = expanded ? (
    // A button cannot hold links, so the whole note is a plain block; the title stays the button.
    <div className={frame}>
      {bar}
      <div className="min-w-0 flex-1">
        <button onClick={onClick} aria-label={note.title || subtitle ? undefined : t("Edit note")} className="block w-full text-left">
          {head}
        </button>
        {note.content && (
          <div
            className="note-md text-sm text-fg/85"
            onClick={onBodyClick}
            dangerouslySetInnerHTML={{ __html: renderMarkdown(note.content, { links: true }) }}
          />
        )}
        {tags}
      </div>
    </div>
  ) : (
    <button onClick={onClick} className={frame}>
      {bar}
      <div className="min-w-0 flex-1">
        {head}
        {note.content && (
          <div
            ref={body}
            className="note-md line-clamp-4 text-sm text-fg/85"
            dangerouslySetInnerHTML={{ __html: renderMarkdown(note.content, { links: false }) }}
          />
        )}
        {tags}
      </div>
    </button>
  );
  if (!onToggle) return card;
  const label = expanded ? t("Show less") : t("Show whole note");
  return (
    <div className="relative">
      {card}
      {toggle && (
        <button
          title={label}
          aria-label={label}
          aria-expanded={expanded}
          onClick={onToggle}
          className="absolute right-2 top-2 p-1 text-muted hover:text-accent"
        >
          {expanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
      )}
    </div>
  );
}

export type NoteSaveInput = { title: string; content: string; tags: string[] };

/** Saves a draft: an existing note by its guid, a new one on the page and highlight it was started on. */
export const saveDraft = (draft: NoteDraft, input: NoteSaveInput) =>
  api.saveNote(draft.note.guid ? null : (draft.target ?? null), {
    ...input,
    guid: draft.note.guid,
    blockType: draft.blockType ?? 0,
    blockIdentifier: draft.blockIdentifier ?? null,
    markGuid: draft.markGuid ?? null,
  });

/** The fields of a note and its buttons, shared by the study pane's editor and the note window. */
export function NoteForm({
  note,
  initial,
  focus = true,
  onSave,
  onDelete,
  onCancel,
  onDirty,
  onChange,
  onError,
  onAppLink,
  onOpenLink,
  books = null,
}: {
  note: Partial<Note>;
  /** Fields to start with instead of the note's, such as unsaved text moved from another editor. */
  initial?: NoteFields;
  /** Focus the title once this is true; a hidden field cannot take focus. */
  focus?: boolean;
  onSave: (input: NoteSaveInput) => void;
  onDelete?: () => void;
  onCancel: () => void;
  /** Called with whether the fields differ from the note as opened. */
  onDirty?: (dirty: boolean) => void;
  onChange?: (fields: NoteFields) => void;
  onError?: (message: string) => void;
  /** Opens the app's own links in the preview, such as a Bible verse; without it the main window opens them. */
  onAppLink?: (href: string) => void;
  /** Opens the verses Ctrl+P links; without it Ctrl+P does nothing. */
  onOpenLink?: (href: string) => void;
  /** The books of the Bible that Ctrl+L links references to; without them it links no reference. */
  books?: Book[] | null;
}) {
  const start = initial ?? fieldsOf(note);
  const [title, setTitle] = useState(start.title);
  const [content, setContent] = useState(start.content);
  const [tags, setTags] = useState(start.tags);
  const [preview, setPreview] = useState(false);
  const titleRef = useRef<HTMLInputElement>(null);
  const contentRef = useRef<HTMLTextAreaElement>(null);
  /** Where the caret goes once a link set without the browser's editing is shown. */
  const caret = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (caret.current == null) return;
    contentRef.current?.setSelectionRange(caret.current, caret.current);
    caret.current = null;
  }, [content]);
  const dirty = !sameFields({ title, content, tags }, fieldsOf(note));
  useEffect(() => onDirty?.(dirty), [dirty, onDirty]);
  useEffect(() => onChange?.({ title, content, tags }), [title, content, tags, onChange]);
  useEffect(() => {
    if (focus) titleRef.current?.focus();
  }, [focus]);
  /**
   * Ctrl+L, or Cmd+L on a Mac, makes the selection a link; a Bible reference links to its verses. Ctrl+P does the same
   * and opens the first verses linked, where the form can open them.
   */
  const onContentKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey || e.nativeEvent.isComposing) return;
    const key = e.key.toLowerCase();
    if (key !== "l" && !(key === "p" && onOpenLink)) return;
    e.preventDefault();
    const area = e.currentTarget;
    const edit = linkEdit(area.value, area.selectionStart, area.selectionEnd, books);
    area.setSelectionRange(edit.start, edit.end);
    // Typed through the browser, the link can be undone; where that is missing, the text is set directly.
    if (document.execCommand?.("insertText", false, edit.text)) {
      area.setSelectionRange(edit.caret, edit.caret);
    } else {
      caret.current = edit.caret;
      setContent(area.value.slice(0, edit.start) + edit.text + area.value.slice(edit.end));
    }
    if (key === "p" && edit.href) onOpenLink?.(edit.href);
  };
  /** Links in the preview open in the app or the browser, not in the editor's window. */
  const openLink = (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    // A host around the form, such as the study pane, must not handle the same click.
    e.stopPropagation();
    openNoteLink(a, onAppLink, (message) => onError?.(message));
  };
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Input
        id="note-title"
        ref={titleRef}
        placeholder={t("Title")}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        className="h-auto py-2 text-base"
      />
      <div role="group" aria-label={t("Note")} className="-mb-1.5 flex items-center justify-end gap-1 text-xs">
        {!preview && (
          <span className="mr-auto text-muted">
            {t("Ctrl+L: link a Bible reference")}
            {onOpenLink && (
              <>
                {" · "}
                <span>{t("Ctrl+P: link and open a Bible reference")}</span>
              </>
            )}
          </span>
        )}
        {[false, true].map((on) => (
          <button
            key={String(on)}
            type="button"
            aria-pressed={preview === on}
            onClick={() => setPreview(on)}
            className="rounded px-2 py-0.5 text-muted hover:text-fg aria-pressed:bg-bar aria-pressed:font-medium aria-pressed:text-fg"
          >
            {on ? t("Preview") : t("Write")}
          </button>
        ))}
      </div>
      {preview ? (
        <div
          data-testid="note-preview"
          onClick={openLink}
          className="note-md min-h-0 flex-1 overflow-auto rounded-md border border-line px-3 py-2"
          dangerouslySetInnerHTML={{
            __html: renderMarkdown(content, { links: true }) || `<p class="text-muted">${t("Nothing to preview")}</p>`,
          }}
        />
      ) : (
        <Textarea
          id="note-content"
          ref={contentRef}
          placeholder={t("Note")}
          rows={6}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onKeyDown={onContentKey}
          className="min-h-0 flex-1 resize-none text-base"
        />
      )}
      <Input
        id="note-tags"
        placeholder={t("Tags, separated by commas")}
        value={tags}
        onChange={(e) => setTags(e.target.value)}
        className="h-auto py-2"
      />
      <div className="flex items-center gap-2">
        {onDelete && (
          <Button variant="ghost" onClick={onDelete} className="px-3 text-muted hover:text-fg">
            <Trash2 size={16} /> {t("Delete")}
          </Button>
        )}
        <span className="flex-1" />
        <Button variant="ghost" onClick={onCancel}>
          {t("Cancel")}
        </Button>
        <Button
          disabled={!title.trim() && !content.trim()}
          onClick={() => onSave({ title: title.trim(), content, tags: parseTags(tags) })}
        >
          {t("Save")}
        </Button>
      </div>
    </div>
  );
}

/** Whether a key press asks to move the note to its window: Ctrl+Shift+O, or Cmd+Shift+O on a Mac. */
const isPopOut = (e: React.KeyboardEvent) => (e.ctrlKey || e.metaKey) && e.shiftKey && !e.altKey && e.key.toLowerCase() === "o";

/**
 * The note being written in the reader's study pane, in place of its card. It
 * saves to the page the note was started on and can move to the note window.
 * The app's own links in its preview open with `onAppLink`, else in the app.
 */
export function InlineNoteEditor({ onAppLink }: { onAppLink?: (href: string) => void }) {
  const { note, closeNote, setNoteDirty, setNoteFields, popOutNote, notesChanged, toast, view, publications, lang, bibles, openLink } =
    useApp();
  /** Set while a save or delete runs, so a second click does not save the note twice. */
  const busy = useRef(false);
  const bible = useLinkBible(view.name === "reader" ? view.target.publication : null, publications, lang, bibles);
  if (!note || note.moving) return null;
  const { id, draft } = note;
  // The editor stays open until the backend agreed, so a failure does not lose the text.
  const run = (action: () => Promise<unknown>) => {
    if (busy.current) return;
    busy.current = true;
    action()
      .then(() => {
        notesChanged();
        return closeNote(id, { force: true });
      })
      .catch((e) => toast(String(e)))
      .finally(() => (busy.current = false));
  };
  const label = draft.note.guid ? t("Edit note") : t("New note");
  return (
    <section
      role="form"
      aria-label={label}
      data-note-editor
      onKeyDown={(e) => {
        if (canOpenNoteWindow() && isPopOut(e)) {
          e.preventDefault();
          e.stopPropagation();
          void popOutNote();
        } else if (e.key === "Escape") {
          e.stopPropagation();
          void closeNote(id);
        }
      }}
      className="flex min-h-[320px] flex-col gap-3 rounded-md border border-accent bg-surface p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <h4 className="text-sm font-semibold">{label}</h4>
        {canOpenNoteWindow() && (
          <button
            type="button"
            title={t("Open in window (Ctrl+Shift+O)")}
            aria-label={t("Open in window (Ctrl+Shift+O)")}
            onClick={() => void popOutNote()}
            className="p-1 text-muted hover:text-accent"
          >
            <SquareArrowOutUpRight size={16} />
          </button>
        )}
      </div>
      <NoteForm
        key={`${id}.${note.restart ?? 0}`}
        note={draft.note}
        initial={note.fields}
        onDirty={setNoteDirty}
        onChange={setNoteFields}
        onError={toast}
        onAppLink={onAppLink ?? ((href) => void openLink(href))}
        books={bible?.books}
        onCancel={() => void closeNote(id)}
        onSave={(input) => run(() => saveDraft(draft, input))}
        onDelete={draft.note.guid ? () => run(() => api.deleteNote(draft.note.guid!)) : undefined}
      />
    </section>
  );
}

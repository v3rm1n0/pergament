import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { NoteForm, saveDraft, type NoteSaveInput } from "@/components/notes";
import { api, type PubCard } from "@/lib/api";
import { fieldsOf, sameFields } from "@/lib/notes";
import {
  askDiscard,
  isNewMessage,
  NOTE_DIRTY,
  NOTE_OPEN,
  NOTE_READY,
  NOTES_CHANGED,
  showNoteLink,
  type NoteWindowMessage,
} from "@/lib/note-window";
import { useLinkBible } from "@/lib/note-links";
import { LOOK_KEYS, loadBibles, loadLang, loadTheme, loadUiLang, prefersDark, saveNoteWindowSize } from "@/lib/settings";
import { resolveUiLang, setUiLang, t } from "@/lib/i18n";
import { useSystemDark } from "@/lib/theme";

/**
 * The note window: one note at a time, sent by the app. It saves on its own and
 * tells the app, which reloads its notes.
 */
export function NoteWindowView() {
  // The app saves the theme and language to the storage this window shares; follow its changes.
  const [, restyle] = useReducer((n: number) => n + 1, 0);
  useEffect(() => {
    const changed = (e: StorageEvent) => {
      if (e.key === null || LOOK_KEYS.includes(e.key)) restyle();
    };
    window.addEventListener("storage", changed);
    return () => window.removeEventListener("storage", changed);
  }, []);
  setUiLang(resolveUiLang(loadUiLang(), navigator.language));
  const dark = prefersDark(loadTheme(), useSystemDark());
  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
  }, [dark]);

  // One handle, so the effects below subscribe once instead of on every render.
  const win = useMemo(() => getCurrentWebviewWindow(), []);
  const [message, setMessage] = useState<NoteWindowMessage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const shown = useRef<NoteWindowMessage | null>(null);
  const dirty = useRef(false);
  /** Set while a save or delete runs, so a second click does not save the note twice. */
  const busy = useRef(false);
  const [saving, setSaving] = useState<number | null>(null);
  // References link to the Bible the note was started in, else the one the app chose for its language.
  const [publications, setPublications] = useState<PubCard[]>([]);
  useEffect(() => void api.listPublications().then(setPublications, () => undefined), []);
  const bible = useLinkBible(message?.draft.target?.publication ?? null, publications, loadLang(), loadBibles());

  const setDirty = useCallback((d: boolean) => {
    dirty.current = d;
    void emitTo("main", NOTE_DIRTY, d).catch(() => undefined);
  }, []);
  /** Closes without asking; `destroy` skips the close request. */
  const close = useCallback(() => {
    setDirty(false);
    void win.destroy();
  }, [setDirty, win]);
  const show = useCallback(
    (m: NoteWindowMessage) => {
      shown.current = m;
      // Moved text is unsaved from the start, before its form reports so: a note right behind it must ask.
      setDirty(m.fields !== undefined && !sameFields(m.fields, fieldsOf(m.draft.note)));
      setError(null);
      setMessage(m);
    },
    [setDirty],
  );

  /** Set while the discard question is open; a note that comes in meanwhile waits, the newest one wins. */
  const asking = useRef(false);
  const waiting = useRef<NoteWindowMessage | null>(null);
  /** Asks whether the unsaved text may go; also hands back a new note that came in while asking. */
  const question = useCallback(async () => {
    asking.current = true;
    const drop = await askDiscard();
    asking.current = false;
    const next = waiting.current && isNewMessage(shown.current, waiting.current) ? waiting.current : null;
    waiting.current = null;
    return { drop, next };
  }, []);
  /** Shows a note, asking first about unsaved text. When the text is kept, a note that came in meanwhile is asked about next. */
  const take = useCallback(
    async (m: NoteWindowMessage) => {
      if (asking.current) return void (waiting.current = m);
      let next: NoteWindowMessage | null = m;
      while (next && isNewMessage(shown.current, next)) {
        if (!dirty.current) return show(next);
        const asked: NoteWindowMessage = next;
        const answer = await question();
        if (answer.drop) return show(answer.next ?? asked);
        next = answer.next?.id === asked.id ? null : answer.next;
      }
    },
    [question, show],
  );
  /**
   * After the question that came with closing: a note that came in meanwhile was
   * asked for, so dropping the text shows it instead of closing; keeping the text
   * asks about that note next. Returns whether the window may close.
   */
  const closing = useCallback(
    (drop: boolean, next: NoteWindowMessage | null) => {
      if (!next) return drop;
      if (drop) show(next);
      else void take(next);
      return false;
    },
    [show, take],
  );
  const cancel = useCallback(async () => {
    if (!dirty.current) return close();
    if (asking.current) return;
    const { drop, next } = await question();
    if (closing(drop, next)) close();
  }, [close, closing, question]);

  useEffect(() => {
    const un = listen<NoteWindowMessage>(NOTE_OPEN, ({ payload }) => void take(payload));
    // Ask for the note, in case the app sent it before this window listened.
    void un.then(() => emitTo("main", NOTE_READY));
    return () => void un.then((f) => f());
  }, [take]);

  useEffect(() => {
    const closeRequested = win.onCloseRequested(async (e) => {
      if (!dirty.current) return;
      if (asking.current) return e.preventDefault();
      const { drop, next } = await question();
      if (!closing(drop, next)) e.preventDefault();
    });
    const resized = win.onResized(async ({ payload }) => {
      // A maximized window would open that large next time.
      if (await win.isMaximized().catch(() => false)) return;
      const size = payload.toLogical(await win.scaleFactor());
      saveNoteWindowSize({ width: Math.round(size.width), height: Math.round(size.height) });
    });
    return () => {
      void closeRequested.then((f) => f());
      void resized.then((f) => f());
    };
  }, [win, question, closing]);

  // The window stays open until the backend agreed, so a failure does not lose the text.
  const run = (action: () => Promise<unknown>) => {
    if (busy.current) return;
    busy.current = true;
    const id = shown.current?.id;
    setSaving(id ?? null);
    action()
      .then(async () => {
        await emitTo("main", NOTES_CHANGED).catch(() => undefined);
        if (shown.current?.id === id) close();
      })
      .catch((e) => { if (shown.current?.id === id) setError(String(e)); })
      .finally(() => { busy.current = false; setSaving(null); });
  };

  if (!message) return <div className="h-full bg-surface" />;
  const { draft } = message;
  return (
    <div
      role="dialog"
      aria-label={draft.note.guid ? t("Edit note") : t("New note")}
      data-note-editor
      onKeyDown={(e) => {
        if (e.key !== "Escape") return;
        e.preventDefault();
        void cancel();
      }}
      className="flex h-full flex-col gap-3 bg-surface p-4 text-fg"
    >
      <h1 className="font-semibold">{draft.note.guid ? t("Edit note") : t("New note")}</h1>
      {error && (
        <p role="alert" className="text-sm text-red-600 dark:text-red-400">
          {error}
        </p>
      )}
      <NoteForm
        key={message.id}
        disabled={saving === message.id}
        note={draft.note}
        initial={message.fields}
        onDirty={setDirty}
        onError={setError}
        books={bible?.books}
        onOpenLink={showNoteLink}
        onCancel={() => void cancel()}
        onSave={(input: NoteSaveInput) => run(() => saveDraft(draft, input))}
        onDelete={draft.note.guid ? () => run(() => api.deleteNote(draft.note.guid!)) : undefined}
      />
    </div>
  );
}

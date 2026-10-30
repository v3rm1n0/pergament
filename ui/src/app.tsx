import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ask, message as messageDialog, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  ArrowLeft,
  BookOpen,
  Ellipsis,
  FolderInput,
  Gem,
  Home,
  LibraryBig,
  Menu,
  Moon,
  Search,
  Settings,
  Sun,
  Users,
} from "lucide-react";
import { api, samePublication, type PublicationKey, type CatalogEntry, type Language, type Progress, type PubCard, type Target, type UpdateInfo } from "@/lib/api";
import type { MediaRef } from "@/lib/api";
import { fieldsOf, sameFields, type NoteDraft, type NoteFields } from "@/lib/notes";
import { conflictText, mergeToast } from "@/lib/merge";
import type { PlayerItem } from "@/lib/player";
import { linkedItem } from "@/lib/recording";
import {
  askDiscard,
  NOTE_CLOSED,
  NOTE_DIRTY,
  NOTE_LINK,
  NOTE_READY,
  NOTE_SHOW_LINK,
  NOTES_CHANGED,
  sendToNoteWindow,
  type NoteWindowMessage,
} from "@/lib/note-window";
import { DISPLAY_CLOSED, DISPLAY_READY, remember, sendToDisplay, type DisplayMessage } from "@/lib/display";
import { currentView, initialNav, navReducer, type Section, type View } from "@/lib/nav";
import {
  loadFontScale,
  loadLayout,
  loadReadingFont,
  loadMergeBackups,
  loadNoteWindowSize,
  loadSecondDisplay,
  loadUpdateCheck,
  saveSecondDisplay,
  saveUpdateCheck,
  loadBibles,
  loadLang,
  loadUiLang,
  loadTheme,
  prefersDark,
  saveFontScale,
  saveLayout,
  saveReadingFont,
  saveMergeBackups,
  type ReadingFont,
  type Layout,
  saveBibles,
  saveLang,
  saveUiLang,
  saveTheme,
  type Theme,
} from "@/lib/settings";
import { useSystemDark } from "@/lib/theme";
import { cn } from "@/lib/utils";
import { isoDate } from "@/lib/dates";
import { scroller } from "@/lib/scroll";
import { nothingSeen, syncNotices } from "@/lib/sync";
import { resolveUiLang, setUiLang, t, type UiLangSetting } from "@/lib/i18n";
import { LanguageMenu, type Translations } from "@/components/language-menu";
import { useCatalogBibles } from "@/lib/use-catalog-bibles";
import { bibleChoices, bibleFor, bibleStackFor, pickBible } from "@/lib/bible";
import { toast as showToast } from "sonner";
import { Toaster } from "@/components/ui/sonner";

/** The note written in the reader's study pane. */
export interface InlineNote {
  id: number;
  draft: NoteDraft;
  /** Unsaved text the editor starts with instead of the note's: kept while a reference covers the pane, or after moving it to the window failed. */
  fields?: NoteFields;
  /** Counts restarts of the editor, so it takes up `fields` again. */
  restart?: number;
  /** Set while the text moves to the note window; the editor hides meanwhile, so nothing typed gets lost. */
  moving?: boolean;
}

interface Toast {
  text: string;
  action?: { label: string; run: () => void };
}

interface AppContextValue {
  view: View;
  /** The rail section the current view belongs to. */
  section: Section;
  canGoBack: boolean;
  push: (view: View) => void;
  replace: (view: View) => void;
  root: (view: View) => void;
  /** Go to a rail section, back to where the user left it. */
  switchTo: (section: Section, view: View, scroll?: number, stack?: View[]) => void;
  back: () => void;
  openTarget: (target: Target, note?: boolean) => void;
  publications: PubCard[];
  /** Whether the library list has been read at least once. */
  libraryReady: boolean;
  refreshPublications: () => Promise<void>;
  /** Delete a downloaded publication from the library. */
  removePublication: (dir: string, title: string) => Promise<void>;
  toast: (text: string, action?: Toast["action"]) => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
  dark: boolean;
  fontScale: number;
  setFontScale: (n: number) => void;
  layout: Layout;
  setLayout: (l: Layout) => void;
  readingFont: ReadingFont;
  setReadingFont: (f: ReadingFont) => void;
  /** Restoring a backup merges it into the data instead of replacing them. */
  mergeBackups: boolean;
  setMergeBackups: (on: boolean) => void;
  importFiles: () => Promise<void>;
  /** Symbol of the Bible last picked per language code. */
  bibles: Record<string, string>;
  /** Remember which Bible a language's Bible tab opens. */
  chooseBible: (lang: string, symbol: string) => void;
  /** Selected language code, e.g. `X`. */
  lang: string;
  setLang: (code: string) => void;
  /** Interface language setting; the publication language is `lang`. */
  uiLang: UiLangSetting;
  setUiLang: (l: UiLangSetting) => void;
  /** Name of a language in itself, e.g. "Deutsch" for `X`. */
  languageName: (code: string) => string;
  /** Running downloads by `taskKey`. */
  downloads: Record<string, Progress>;
  /** Download and import a catalog publication; resolves to its directory. `lang` defaults to the publication language; `quiet` skips the success toast. */
  download: (entry: CatalogEntry, lang?: string, options?: { quiet?: boolean }) => Promise<string | null>;
  /** Newer versions of downloaded publications, found by checking the catalog. */
  updates: {
    items: UpdateInfo[];
    checking: boolean;
    /** Whether the catalog is checked at startup. */
    enabled: boolean;
    setEnabled: (on: boolean) => void;
    /** Check now; `false` when there is no catalog to check against. */
    check: () => Promise<boolean>;
    update: (u: UpdateInfo) => Promise<void>;
    updateAll: () => Promise<void>;
  };
  /** Bumped whenever the catalog was (re)loaded or the library changed. */
  catalogVersion: number;
  loadCatalog: () => Promise<void>;
  /** Bumped when user data was replaced by a restored backup. */
  userVersion: number;
  /** Bumped when the floating editor saved or deleted a note. */
  notesVersion: number;
  favorites: PublicationKey[];
  setFavorite: (key: PublicationKey, favorite: boolean) => Promise<void>;
  createBackup: () => Promise<void>;
  restoreBackup: () => Promise<void>;
  /** The recording being played; it keeps playing while the user navigates. */
  player: { id: number; item: PlayerItem } | null;
  playItem: (item: PlayerItem) => void;
  /** Look up a recording that a publication links to and play it. */
  playRecording: (media: MediaRef) => Promise<void>;
  /** Open one of the app's own links, such as a Bible verse in a note, in the Bible chosen for the language. */
  openLink: (href: string) => Promise<void>;
  /**
   * Let a view open the app's links clicked in the note window itself, such as the reader in its reference pane;
   * null hands them back to `openLink`.
   */
  setLinkHandler: (handler: ((href: string) => void) | null) => void;
  closePlayer: () => void;
  /** The note written in the reader's study pane. It stays open while the user navigates. */
  note: InlineNote | null;
  /** Open a note to write: in the reader's study pane, or in the note window (the default). */
  openNote: (draft: NoteDraft, options?: { in?: "pane" | "window" }) => Promise<void>;
  /**
   * Close the editor, asking first when that would drop unsaved text (`force`
   * skips that, after a save). With an `id`, only that note is closed.
   */
  closeNote: (id?: number, options?: { force?: boolean }) => Promise<void>;
  /** Told by the editor whether it holds unsaved text. */
  setNoteDirty: (dirty: boolean) => void;
  /** Whether the study pane or the note window holds unsaved text. */
  hasUnsavedNote: () => boolean;
  /** The inline editor reports its fields. */
  setNoteFields: (fields: NoteFields) => void;
  /** Move the inline draft, unsaved text included, to the note window. */
  popOutNote: () => Promise<void>;
  /** The inline editor's place went away: unsaved text goes to the window, an unchanged note is closed. */
  releaseNote: () => void;
  /** The inline editor hides for a while, such as behind a followed reference: it comes back with the text it holds. */
  keepNote: () => void;
  /** Tell the views that a note was saved or deleted. */
  notesChanged: () => void;
  /** The second display window and what is sent to it. */
  display: { enabled: boolean; setEnabled: (on: boolean) => Promise<void>; send: (message: DisplayMessage) => void };
  /** The picture in the large view, if one is open. */
  viewer: { src: string; caption: string } | null;
  /** Open a `jwmedia:` picture in the large view (and on the second display). */
  showImage: (src: string, caption: string) => void;
  closeImage: () => void;
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppProvider");
  return ctx;
}

/** Progress key of a publication download; the same as the backend's (`download_publication`), language included. */
export const taskKey = (e: CatalogEntry) => `download:${e.item.meps_language}:${e.item.symbol}:${e.item.issue_tag}`;

export function AppProvider({ children }: { children: ReactNode }) {
  const [nav, dispatch] = useReducer(navReducer, initialNav);
  const [publications, setPublications] = useState<PubCard[]>([]);
  const [libraryReady, setLibraryReady] = useState(false);
  const [theme, setThemeState] = useState<Theme>(loadTheme);
  const [fontScale, setFontScaleState] = useState(loadFontScale);
  const [layout, setLayoutState] = useState<Layout>(loadLayout);
  const [readingFont, setReadingFontState] = useState<ReadingFont>(loadReadingFont);
  const [mergeBackups, setMergeBackupsState] = useState(loadMergeBackups);
  const [lang, setLangState] = useState(loadLang);
  const [bibles, setBibles] = useState(loadBibles);
  const [uiLang, setUiLangState] = useState<UiLangSetting>(loadUiLang);
  const [languages, setLanguages] = useState<Language[]>([]);
  const [downloads, setDownloads] = useState<Record<string, Progress>>({});
  const [catalogVersion, setCatalogVersion] = useState(0);
  const [userVersion, setUserVersion] = useState(0);
  const [favorites, setFavorites] = useState<PublicationKey[]>([]);
  const favoriteWrites = useRef<PublicationKey[]>([]);
  const favoriteRevision = useRef(0);
  const [player, setPlayer] = useState<AppContextValue["player"]>(null);
  const [note, setNote] = useState<AppContextValue["note"]>(null);
  const noteRef = useRef(note);
  noteRef.current = note;
  const noteDirty = useRef(false);
  const systemDark = useSystemDark();
  const dark = prefersDark(theme, systemDark);
  // Set while rendering so every `t()` below already uses the new language.
  const resolvedUi = resolveUiLang(uiLang, navigator.language);
  setUiLang(resolvedUi);

  useEffect(() => {
    document.documentElement.lang = resolvedUi;
  }, [resolvedUi]);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
  }, [dark]);
  useEffect(() => {
    document.documentElement.style.setProperty("--font-scale", String(fontScale));
  }, [fontScale]);
  useEffect(() => {
    document.documentElement.dataset.font = readingFont;
  }, [readingFont]);
  useEffect(() => {
    // The widest a page column gets; "wide" leaves only the side padding.
    document.documentElement.style.setProperty("--page-max", layout === "wide" ? "100000px" : "880px");
  }, [layout]);
  useEffect(() => {
    api.languages().then(setLanguages).catch(() => setLanguages([]));
  }, []);
  useEffect(() => {
    const un = listen<Progress>("progress", (e) => {
      if (e.payload.task.startsWith("download:")) {
        setDownloads((d) => (d[e.payload.task] ? { ...d, [e.payload.task]: e.payload } : d));
      }
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  // A toast with an action stays longer, to give time to use it.
  const toast = useCallback((text: string, action?: Toast["action"]) => {
    showToast(text, {
      duration: action ? 9000 : 5000,
      action: action && { label: action.label, onClick: action.run },
    });
  }, []);

  useEffect(() => {
    let live = true;
    const revision = favoriteRevision.current;
    api.favorites().then((rows) => {
      if (live && revision === favoriteRevision.current) setFavorites(rows);
    }).catch((e) => toast(String(e)));
    return () => { live = false; };
  }, [userVersion, toast]);

  const setFavorite = useCallback(async ({ keySymbol, mepsLanguage, issueTag }: PublicationKey, favorite: boolean) => {
    const key = { keySymbol, mepsLanguage, issueTag };
    if (favoriteWrites.current.some((k) => samePublication(k, key))) return;
    favoriteWrites.current.push(key);
    favoriteRevision.current++;
    try {
      await api.setFavorite(key, favorite);
      setFavorites((rows) => favorite
        ? rows.some((k) => samePublication(k, key)) ? rows : [...rows, key]
        : rows.filter((k) => !samePublication(k, key)));
    } catch (e) {
      toast(String(e));
    } finally {
      favoriteWrites.current = favoriteWrites.current.filter((k) => !samePublication(k, key));
      favoriteRevision.current++;
      setUserVersion((v) => v + 1);
    }
  }, [toast]);

  const refreshPublications = useCallback(async () => {
    try {
      setPublications(await api.listPublications());
      setLibraryReady(true);
    } catch (e) {
      toast(t("Cannot read library: {error}", { error: String(e) }));
    }
  }, [toast]);

  useEffect(() => {
    void refreshPublications();
  }, [refreshPublications]);

  const removePublication = useCallback(
    async (dir: string, title: string) => {
      try {
        await api.removePublication(dir);
        toast(t("Removed {title}", { title }));
      } catch (e) {
        toast(t("Remove failed: {error}", { error: String(e) }));
      }
      await refreshPublications();
      // Catalog tiles show whether a publication is downloaded.
      setCatalogVersion((v) => v + 1);
    },
    [refreshPublications, toast],
  );

  const importFiles = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      directory: false,
      filters: [{ name: "JW publications", extensions: ["jwpub"] }],
    });
    const paths = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
    if (paths.length === 0) return;
    toast(t("Importing {count} file(s)…", { count: paths.length }));
    try {
      for (const r of await api.importFiles(paths)) {
        toast(r.error ? t("Import failed: {error}", { error: r.error }) : t("Imported {title}", { title: r.title ?? "" }));
      }
    } catch (e) {
      toast(t("Import failed: {error}", { error: String(e) }));
    }
    await refreshPublications();
    setCatalogVersion((v) => v + 1);
  }, [refreshPublications, toast]);

  const langRef = useRef(lang);
  langRef.current = lang;
  const download = useCallback(
    async (entry: CatalogEntry, lang?: string, options?: { quiet?: boolean }) => {
      const key = taskKey(entry);
      setDownloads((d) => ({ ...d, [key]: { task: key, done: 0, total: entry.item.size } }));
      try {
        const dir = await api.downloadPublication(entry.item, lang ?? langRef.current);
        if (!options?.quiet) toast(t("Imported {title}", { title: entry.item.issue_title || entry.item.title }));
        await refreshPublications();
        setCatalogVersion((v) => v + 1);
        return dir;
      } catch (e) {
        toast(t("Download failed: {error}", { error: String(e) }));
        return null;
      } finally {
        setDownloads((d) => {
          const { [key]: _, ...rest } = d;
          return rest;
        });
      }
    },
    [refreshPublications, toast],
  );

  // Updates: the catalog lists a different version of a downloaded publication.
  const [updateItems, setUpdateItems] = useState<UpdateInfo[]>([]);
  const updateItemsRef = useRef(updateItems);
  updateItemsRef.current = updateItems;
  const [checking, setChecking] = useState(false);
  const [updateCheck, setUpdateCheck] = useState(loadUpdateCheck);
  const checkUpdates = useCallback(async (): Promise<boolean> => {
    setChecking(true);
    try {
      const found = await api.checkUpdates();
      if (found === null) return false;
      setUpdateItems(found);
      // The catalog may have been refreshed by the check.
      setCatalogVersion((v) => v + 1);
      return true;
    } catch (e) {
      toast(t("Update check failed: {error}", { error: String(e) }));
      return false;
    } finally {
      setChecking(false);
    }
  }, [toast]);
  const updateOne = useCallback(
    async (u: UpdateInfo) => {
      const dir = await download(u.entry, u.langCode);
      if (dir) setUpdateItems((items) => items.filter((x) => x.dir !== u.dir));
    },
    [download],
  );
  const updateAll = useCallback(async () => {
    for (const u of updateItemsRef.current) await updateOne(u);
  }, [updateOne]);
  // Once at startup, if the catalog is cached already and the setting allows it.
  const startupChecked = useRef(false);
  useEffect(() => {
    if (startupChecked.current || !loadUpdateCheck()) return;
    startupChecked.current = true;
    setChecking(true);
    api
      .checkUpdates()
      .then((found) => {
        if (!found) return;
        setUpdateItems(found);
        if (found.length > 0) {
          toast(
            found.length === 1
              ? t("1 publication update is available")
              : t("{count} publication updates are available", { count: found.length }),
            { label: t("Show"), run: () => dispatch({ type: "push", view: { name: "library", tab: "updates" } }) },
          );
        }
      })
      .catch(() => undefined)
      .finally(() => setChecking(false));
  }, [toast]);

  const loadCatalog = useCallback(async () => {
    try {
      await api.loadCatalog();
      setCatalogVersion((v) => v + 1);
      // A catalog that was just loaded can tell what has updates.
      void checkUpdates();
    } catch (e) {
      toast(t("Catalog: {error}", { error: String(e) }));
    }
  }, [toast, checkUpdates]);

  const createBackup = useCallback(async () => {
    const path = await saveDialog({
      defaultPath: `UserdataBackup_${isoDate(new Date())}_Pergament.jwlibrary`,
      filters: [{ name: "Backup", extensions: ["jwlibrary"] }],
    });
    if (!path) return;
    try {
      await api.exportBackup(path);
      toast(t("Backup created"));
    } catch (e) {
      toast(t("Backup failed: {error}", { error: String(e) }));
    }
  }, [toast]);

  // A restore or merge, from choosing the file to the last dialog, runs once at a time.
  const restoring = useRef(false);
  const restoreBackup = useCallback(async () => {
    if (restoring.current) return;
    restoring.current = true;
    try {
      const picked = await openDialog({ multiple: false, filters: [{ name: "Backup", extensions: ["jwlibrary"] }] });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      if (mergeBackups) {
        const ok = await ask(
          t(
            "Merging adds what is only in the backup and keeps the newer version of everything that is in both. Nothing is deleted. Your current data is kept as userData.db.before-restore.",
          ),
          { title: t("Merge backup?"), kind: "info", okLabel: t("Merge") },
        );
        if (!ok) return;
        let report;
        try {
          report = await api.mergeBackup(path);
        } catch (e) {
          toast(t("Merge failed: {error}", { error: String(e) }));
          return;
        }
        setUserVersion((v) => v + 1);
        toast(mergeToast(report));
        if (report.conflicts.length > 0) {
          // The merge is done either way; without a dialog the list goes to a toast.
          const text = conflictText(report.conflicts);
          try {
            await messageDialog(text, { title: t("Conflicts"), kind: "warning" });
          } catch {
            toast(text);
          }
        }
        return;
      }
      // Restoring spreads to every synced computer, so when the settings cannot be read, warn anyway.
      const syncing = await api.autoSettings().then(
        (a) => a.sync.folder !== null,
        () => true,
      );
      const warning = t("Restoring replaces all favorites, highlights, notes, tags and bookmarks with those in the backup.");
      const ok = await ask(syncing ? `${warning} ${t("Sync is on: this also changes your data on the other synced computers.")}` : warning, {
        title: t("Restore backup?"),
        kind: "warning",
        okLabel: t("Restore"),
      });
      if (!ok) return;
      try {
        const s = await api.restoreBackup(path);
        toast(
          t("Restored {marks} highlights and {notes} notes", { marks: s.marks, notes: s.notes }) +
            (s.device ? t(" from {device}", { device: s.device }) : ""),
        );
        setUserVersion((v) => v + 1);
      } catch (e) {
        toast(t("Restore failed: {error}", { error: String(e) }));
      }
    } finally {
      restoring.current = false;
    }
  }, [toast, mergeBackups]);

  const names = useMemo(() => new Map(languages.map((l) => [l.code, l.vernacular])), [languages]);

  const playerRef = useRef(player);
  playerRef.current = player;

  // The second display: a window the app sends what to show; one that opens
  // later is brought up to date with the last thing sent.
  const [displayOn, setDisplayOn] = useState(loadSecondDisplay);
  const displayOnRef = useRef(displayOn);
  const lastDisplay = useRef<DisplayMessage>({ type: "idle" });
  const turnDisplayOff = useCallback(() => {
    saveSecondDisplay(false);
    displayOnRef.current = false;
    setDisplayOn(false);
  }, []);
  const sendDisplay = useCallback((message: DisplayMessage) => {
    lastDisplay.current = remember(lastDisplay.current, message);
    if (displayOnRef.current) void sendToDisplay(message);
  }, []);
  const setSecondDisplay = useCallback(
    async (on: boolean) => {
      saveSecondDisplay(on);
      displayOnRef.current = on;
      setDisplayOn(on);
      try {
        await (on ? api.openDisplay() : api.closeDisplay());
      } catch (e) {
        toast(t("Cannot open the second display: {error}", { error: String(e) }));
        if (on) turnDisplayOff();
      }
    },
    [toast, turnDisplayOff],
  );
  useEffect(() => {
    if (loadSecondDisplay()) {
      api.openDisplay().catch((e) => {
        toast(t("Cannot open the second display: {error}", { error: String(e) }));
        turnDisplayOff();
      });
    }
    const ready = listen(DISPLAY_READY, () => void sendToDisplay(lastDisplay.current));
    const closed = listen(DISPLAY_CLOSED, turnDisplayOff);
    return () => {
      void ready.then((f) => f());
      void closed.then((f) => f());
    };
    // Runs once: it restores the setting from the last session.
  }, []);

  // A sync merge changed the data: reload what the views show.
  useEffect(() => {
    const changed = listen("user-data-changed", () => setUserVersion((v) => v + 1));
    return () => void changed.then((f) => f());
  }, []);

  // A peer waiting for a choice or a failing sync folder shows up in Settings only, so say so once in a toast.
  const syncSeen = useRef(nothingSeen);
  const syncOn = useRef(false);
  useEffect(() => {
    let alive = true;
    const check = () =>
      api.syncStatus().then(
        (status) => {
          if (!alive) return;
          syncOn.current = Boolean(status.folder);
          const { notices, seen } = syncNotices(syncSeen.current, status);
          syncSeen.current = seen;
          notices.forEach((text) => toast(text));
        },
        () => undefined,
      );
    void check();
    const changed = listen("user-data-changed", () => void check());
    const onFocus = () => void check();
    window.addEventListener("focus", onFocus);
    const timer = setInterval(() => syncOn.current && void check(), 60_000);
    return () => {
      alive = false;
      void changed.then((f) => f());
      window.removeEventListener("focus", onFocus);
      clearInterval(timer);
    };
  }, [toast]);

  const [viewer, setViewer] = useState<AppContextValue["viewer"]>(null);
  const showImage = useCallback(
    (src: string, caption: string) => {
      setViewer({ src, caption });
      sendDisplay({ type: "image", src, caption });
    },
    [sendDisplay],
  );
  const closeImage = useCallback(() => {
    setViewer(null);
    // With a recording playing, its player takes the display back.
    if (!playerRef.current) sendDisplay({ type: "idle" });
  }, [sendDisplay]);

  const playItem = useCallback((item: PlayerItem) => setPlayer({ id: Date.now(), item }), []);
  const closePlayer = useCallback(() => setPlayer(null), []);
  /** Whether unsaved text in the editor may be dropped: true when there is none or the user agrees. */
  const mayDiscard = useCallback(async () => !noteDirty.current || (await askDiscard()), []);
  const [notesVersion, setNotesVersion] = useState(0);
  const notesChanged = useCallback(() => setNotesVersion((v) => v + 1), []);
  // The note window: the app sends it a note; one that opens later asks for it when ready.
  const lastNoteMessage = useRef<NoteWindowMessage | null>(null);
  const noteWindowDirty = useRef(false);
  const hasUnsavedNote = useCallback(() => noteDirty.current || noteWindowDirty.current, []);
  /** Set while moved text is on its way to the window: a note sent after it would take its place unseen. */
  const movedPending = useRef(false);
  const showInWindow = useCallback(
    async (message: NoteWindowMessage): Promise<boolean> => {
      lastNoteMessage.current = message;
      try {
        const size = loadNoteWindowSize();
        await api.openNoteWindow(size?.width, size?.height);
        void sendToNoteWindow(message);
        return true;
      } catch (e) {
        lastNoteMessage.current = null;
        toast(t("Cannot open the note window: {error}", { error: String(e) }));
        return false;
      }
    },
    [toast],
  );
  /** The note window holds unsaved text, or moved text on its way there: say so and bring it forward instead. */
  const noteWindowBusy = useCallback(() => {
    toast(t("Save or close the note in the note window first."));
    const size = loadNoteWindowSize();
    void api.openNoteWindow(size?.width, size?.height).catch(() => undefined);
  }, [toast]);
  useEffect(() => {
    const subscriptions = [
      listen(NOTE_READY, () => {
        if (lastNoteMessage.current) void sendToNoteWindow(lastNoteMessage.current);
      }),
      listen<boolean>(NOTE_DIRTY, (e) => {
        noteWindowDirty.current = e.payload;
        // The window shows the moved text and asks before anything replaces it.
        if (e.payload) movedPending.current = false;
      }),
      listen(NOTES_CHANGED, notesChanged),
      listen(NOTE_CLOSED, () => {
        lastNoteMessage.current = null;
        noteWindowDirty.current = false;
        movedPending.current = false;
      }),
    ];
    return () => subscriptions.forEach((s) => void s.then((f) => f()));
  }, [notesChanged]);
  // Closing the app closes the note window too, so ask first when either editor holds unsaved text.
  useEffect(() => {
    const closing = getCurrentWebviewWindow().onCloseRequested(async (e) => {
      if (!noteDirty.current && !noteWindowDirty.current) return;
      if (!(await askDiscard())) e.preventDefault();
    });
    return () => void closing.then((f) => f());
  }, []);

  /** What the study pane's editor holds now, to move it to the note window. */
  const noteFields = useRef<NoteFields | null>(null);
  const setNoteFields = useCallback((fields: NoteFields) => {
    noteFields.current = fields;
  }, []);
  /** The id of the note moving to the window, so a second request does not send it twice. */
  const moving = useRef<number | null>(null);
  /**
   * Moves the inline draft to the window. `release`: its place went away, and
   * the editor that took its new place started over with the note's own text.
   */
  const moveToWindow = useCallback(
    async (release: boolean) => {
      const current = noteRef.current;
      if (!current || moving.current === current.id) return;
      // Taken now, before an editor in a new place reports the note's own fields.
      const fields = noteFields.current ?? undefined;
      /** Shows the editor again with the text it held, still unsaved. */
      const restore = () => {
        noteDirty.current = true;
        noteFields.current = fields ?? null;
        setNote({ id: current.id, draft: current.draft, fields, restart: (current.restart ?? 0) + 1 });
      };
      // The window would ask about its own text and could drop this one; the draft stays here.
      if (noteWindowDirty.current) {
        noteWindowBusy();
        if (release) restore();
        return;
      }
      moving.current = current.id;
      const changed = fields !== undefined && !sameFields(fields, fieldsOf(current.draft.note));
      // The window holds this text from now on, before it can report so: a second move must not replace it.
      if (changed) noteDirty.current = noteWindowDirty.current = movedPending.current = true;
      setNote({ ...current, moving: true });
      const opened = await showInWindow({ id: Date.now(), draft: current.draft, fields });
      moving.current = null;
      if (!opened && changed) noteWindowDirty.current = movedPending.current = false;
      // If another note replaced it meanwhile, leave that one alone.
      if (noteRef.current?.id !== current.id) return;
      if (!opened) return restore();
      noteDirty.current = false;
      noteFields.current = null;
      setNote(null);
    },
    [noteWindowBusy, showInWindow],
  );
  const popOutNote = useCallback(() => moveToWindow(false), [moveToWindow]);
  const releaseNote = useCallback(() => {
    if (!noteRef.current || moving.current !== null) return;
    if (noteDirty.current) return void moveToWindow(true);
    noteFields.current = null;
    setNote(null);
  }, [moveToWindow]);
  const keepNote = useCallback(() => {
    const current = noteRef.current;
    if (!current || moving.current !== null || !noteFields.current) return;
    setNote({ ...current, fields: noteFields.current });
  }, []);
  const openNote = useCallback(
    async (draft: NoteDraft, { in: where = "window" }: { in?: "pane" | "window" } = {}) => {
      if (where === "window") {
        // Moved text the window has not taken yet would be replaced without a question; its own text it asks about.
        if (movedPending.current) return noteWindowBusy();
        return void (await showInWindow({ id: Date.now(), draft }));
      }
      if (!(await mayDiscard())) return;
      noteDirty.current = false;
      noteFields.current = null;
      setNote({ id: Date.now(), draft });
    },
    [mayDiscard, noteWindowBusy, showInWindow],
  );
  const closeNote = useCallback(
    async (id?: number, { force = false } = {}) => {
      // A save that finishes after another note was opened must not touch that one.
      const current = () => id === undefined || noteRef.current?.id === id;
      if (!current() || (!force && !(await mayDiscard())) || !current()) return;
      noteDirty.current = false;
      noteFields.current = null;
      setNote(null);
    },
    [mayDiscard],
  );
  const setNoteDirty = useCallback((dirty: boolean) => {
    // An editor that took the moving note's place reports the note's own fields, not the moving text;
    // a note opened meanwhile reports its own.
    if (moving.current === null || noteRef.current?.id !== moving.current) noteDirty.current = dirty;
  }, []);
  const playRecording = useCallback(
    async (media: MediaRef) => {
      try {
        playItem(await linkedItem(media));
      } catch (e) {
        toast(String(e instanceof Error ? e.message : e));
      }
    },
    [playItem, toast],
  );
  const openLink = useCallback(
    async (href: string) => {
      try {
        const action = await api.linkAction(bibleFor(null, publications, lang, bibles)?.dir ?? null, href);
        switch (action.kind) {
          case "open":
            dispatch({ type: "push", view: { name: "reader", target: action.target } });
            break;
          case "external":
            await api.openExternal(action.url);
            break;
          case "media":
            await playRecording(action.media);
            break;
          // Only the reader shows excerpts; elsewhere the quoted publication is simply missing.
          case "missing":
          case "extract":
            toast(t("Not in your library: {link}", { link: action.url ?? href }));
            break;
          case "ignore":
            break;
          default:
            action satisfies never;
        }
      } catch (e) {
        toast(t("Cannot follow link: {error}", { error: String(e) }));
      }
    },
    [publications, lang, bibles, playRecording, toast],
  );
  const linkHandler = useRef<((href: string) => void) | null>(null);
  const setLinkHandler = useCallback((handler: ((href: string) => void) | null) => {
    linkHandler.current = handler;
  }, []);
  // The latest opener, so the note window's links are heard all along, not only between library changes.
  const latestOpenLink = useRef(openLink);
  latestOpenLink.current = openLink;
  // A link clicked in the note window opens here, brought in front of it; the note window stays open. One linked and
  // opened with Ctrl+P is shown behind it, so the note can be written on.
  useEffect(() => {
    const follow = (payload: unknown) => {
      if (typeof payload !== "string" || !payload.startsWith("pergament://")) return false;
      if (linkHandler.current) linkHandler.current(payload);
      else void latestOpenLink.current(payload);
      return true;
    };
    const clicked = listen<unknown>(NOTE_LINK, ({ payload }) => {
      if (!follow(payload)) return;
      const win = getCurrentWebviewWindow();
      void win
        .unminimize()
        .then(() => win.setFocus())
        .catch(() => undefined);
    });
    const shown = listen<unknown>(NOTE_SHOW_LINK, ({ payload }) => void follow(payload));
    return () => {
      void clicked.then((f) => f());
      void shown.then((f) => f());
    };
  }, []);

  const value = useMemo<AppContextValue>(
    () => ({
      view: currentView(nav),
      section: nav.section,
      canGoBack: nav.stack.length > 1,
      push: (view) => dispatch({ type: "push", view }),
      replace: (view) => dispatch({ type: "replace", view }),
      root: (view) => dispatch({ type: "root", view }),
      switchTo: (section, view, scroll, stack) => dispatch({ type: "switch", to: section, view, scroll, stack }),
      back: () => dispatch({ type: "back" }),
      openTarget: (target, note) => dispatch({ type: "push", view: { name: "reader", target, note } }),
      publications,
      libraryReady,
      refreshPublications,
      removePublication,
      toast,
      theme,
      setTheme: (t) => {
        saveTheme(t);
        setThemeState(t);
      },
      dark,
      fontScale,
      setFontScale: (n) => {
        saveFontScale(n);
        setFontScaleState(n);
      },
      layout,
      setLayout: (l) => {
        saveLayout(l);
        setLayoutState(l);
      },
      readingFont,
      setReadingFont: (f) => {
        saveReadingFont(f);
        setReadingFontState(f);
      },
      mergeBackups,
      setMergeBackups: (on) => {
        saveMergeBackups(on);
        setMergeBackupsState(on);
      },
      importFiles,
      bibles,
      chooseBible: (code, symbol) =>
        setBibles((b) => {
          const next = { ...b, [code]: symbol };
          saveBibles(next);
          return next;
        }),
      lang,
      setLang: (code) => {
        saveLang(code);
        setLangState(code);
      },
      uiLang,
      setUiLang: (l) => {
        saveUiLang(l);
        setUiLangState(l);
      },
      languageName: (code) => names.get(code) ?? code,
      downloads,
      download,
      updates: {
        items: updateItems,
        checking,
        enabled: updateCheck,
        setEnabled: (on) => {
          saveUpdateCheck(on);
          setUpdateCheck(on);
        },
        check: checkUpdates,
        update: updateOne,
        updateAll,
      },
      catalogVersion,
      loadCatalog,
      userVersion,
      notesVersion,
      favorites,
      setFavorite,
      createBackup,
      restoreBackup,
      player,
      playItem,
      playRecording,
      openLink,
      setLinkHandler,
      closePlayer,
      note,
      openNote,
      closeNote,
      setNoteDirty,
      hasUnsavedNote,
      setNoteFields,
      popOutNote,
      releaseNote,
      keepNote,
      notesChanged,
      display: { enabled: displayOn, setEnabled: setSecondDisplay, send: sendDisplay },
      viewer,
      showImage,
      closeImage,
    }),
    [
      nav,
      publications,
      libraryReady,
      refreshPublications,
      removePublication,
      toast,
      theme,
      dark,
      fontScale,
      layout,
      readingFont,
      mergeBackups,
      importFiles,
      lang,
      bibles,
      nav.section,
      uiLang,
      names,
      downloads,
      download,
      updateItems,
      checking,
      updateCheck,
      checkUpdates,
      updateOne,
      updateAll,
      catalogVersion,
      loadCatalog,
      userVersion,
      notesVersion,
      favorites,
      setFavorite,
      createBackup,
      restoreBackup,
      player,
      playItem,
      playRecording,
      openLink,
      setLinkHandler,
      closePlayer,
      note,
      openNote,
      closeNote,
      setNoteDirty,
      hasUnsavedNote,
      setNoteFields,
      popOutNote,
      releaseNote,
      keepNote,
      notesChanged,
      displayOn,
      setSecondDisplay,
      sendDisplay,
      viewer,
      showImage,
      closeImage,
    ],
  );

  return (
    <AppContext.Provider value={value}>
      {children}
      <Toaster />
    </AppContext.Provider>
  );
}

/** Purple strip across the top with the back arrow, like a window title. */
export function TitleStrip() {
  const { canGoBack, back } = useApp();
  return (
    <div data-tauri-drag-region className="flex h-7 shrink-0 items-center gap-3 border-b border-line bg-surface px-2 text-xs text-muted">
      <button
        aria-label={t("Back")}
        disabled={!canGoBack}
        onClick={back}
        className="flex h-6 w-6 items-center justify-center disabled:opacity-40"
      >
        <ArrowLeft size={14} />
      </button>
      <span data-tauri-drag-region>Pergament</span>
    </div>
  );
}

function RailButton({
  label,
  active,
  onClick,
  expanded,
  toggles,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  expanded: boolean;
  /** The button that shows or hides the labels, which reports its state. */
  toggles?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      title={t(label)}
      aria-label={t(label)}
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      aria-expanded={toggles ? expanded : undefined}
      className={cn(
        "flex h-9 w-full items-center gap-3 rounded-md px-2 text-muted hover:bg-bar hover:text-fg focus-visible:outline-2 focus-visible:outline-accent",
        active && "bg-bar text-fg",
      )}
    >
      <span className="flex w-5 shrink-0 justify-center">{children}</span>
      {expanded && <span className="text-sm">{t(label)}</span>}
    </button>
  );
}

/** Icon rail on the left; the menu button expands labels. */
export function Rail() {
  const { view, switchTo, publications, lang, bibles } = useApp();
  const [expanded, setExpanded] = useState(false);
  const bible = pickBible(bibleChoices(lang, publications, []), bibles[lang] ?? null);
  const inBible =
    (view.name === "publication" || view.name === "chapters") && publications.some((p) => p.isBible && p.dir === view.dir);
  const go = (section: Section, v: View) => {
    setExpanded(false);
    switchTo(section, v, scroller()?.scrollTop);
  };
  const icon = { size: 18, strokeWidth: 1.6 };
  return (
    <nav
      aria-label={t("Main navigation")}
      className={cn("flex shrink-0 flex-col gap-1 border-r border-line bg-surface p-2 transition-[width] motion-reduce:transition-none", expanded ? "w-52" : "w-[52px]")}>
      <RailButton label="Menu" toggles active={false} expanded={expanded} onClick={() => setExpanded((e) => !e)}>
        <Menu {...icon} />
      </RailButton>
      <RailButton label="Home" active={view.name === "home"} expanded={expanded} onClick={() => go("home", { name: "home" })}>
        <Home {...icon} />
      </RailButton>
      <RailButton
        label="Bible"
        active={inBible || view.name === "bible" || (view.name === "reader" && "chapter" in view.target.kind)}
        expanded={expanded}
        onClick={() => go("bible", bible ? { name: "publication", dir: bible.dir! } : { name: "bible" })}
      >
        <BookOpen {...icon} />
      </RailButton>
      <RailButton
        label="Library"
        active={view.name === "library" || view.name === "category" || view.name === "media"}
        expanded={expanded}
        onClick={() => go("library", { name: "library" })}
      >
        <LibraryBig {...icon} />
      </RailButton>
      <RailButton
        label="Meetings"
        active={view.name === "meetings"}
        expanded={expanded}
        onClick={() => go("meetings", { name: "meetings" })}
      >
        <Users {...icon} />
      </RailButton>
      <RailButton
        label="Personal Study"
        active={view.name === "personal"}
        expanded={expanded}
        onClick={() => go("personal", { name: "personal" })}
      >
        <Gem {...icon} />
      </RailButton>
      <div className="flex-1" />
      <RailButton
        label="Settings"
        active={view.name === "settings"}
        expanded={expanded}
        onClick={() => go("settings", { name: "settings" })}
      >
        <Settings {...icon} />
      </RailButton>
    </nav>
  );
}

export function BarButton({
  label,
  onClick,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      title={t(label)}
      aria-label={t(label)}
      disabled={disabled}
      onClick={onClick}
      className="flex size-8 items-center justify-center rounded-md text-muted hover:bg-bar hover:text-fg focus-visible:outline-2 focus-visible:outline-accent disabled:opacity-40"
    >
      {children}
    </button>
  );
}

/** Overflow menu with less frequent actions. */
function MoreMenu() {
  const { importFiles, dark, setTheme, push } = useApp();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);
  const item = (label: string, icon: ReactNode, run: () => void) => (
    <button
      className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm hover:bg-bar"
      onClick={() => {
        setOpen(false);
        run();
      }}
    >
      {icon}
      {t(label)}
    </button>
  );
  return (
    <div ref={ref} className="relative">
      <BarButton label="More" onClick={() => setOpen((o) => !o)}>
        <Ellipsis size={18} strokeWidth={1.6} />
      </BarButton>
      {open && (
        <div className="absolute right-0 top-9 z-40 w-56 rounded-md border border-line bg-surface py-1 shadow-sm">
          {item("Import .jwpub…", <FolderInput size={17} />, () => void importFiles())}
          {item(dark ? "Light mode" : "Dark mode", dark ? <Sun size={17} /> : <Moon size={17} />, () =>
            setTheme(dark ? "light" : "dark"),
          )}
          {item("Settings", <Settings size={17} />, () => push({ name: "settings" }))}
        </div>
      )}
    </div>
  );
}

/**
 * The Bibles each language offers, downloaded or from the catalog, and what picking one does: select its language,
 * remember it for the Bible tab, download it if needed and open it, keeping the place that was open. Only reads the
 * catalog while `enabled`.
 */
export function useBibleTranslations(enabled: boolean): Translations | undefined {
  const { lang, setLang, publications, bibles, chooseBible, download, downloads, switchTo, catalogVersion, view } = useApp();
  const libraryCodes = [...new Set(publications.map((p) => p.langCode).filter((c): c is string => !!c))];
  // Only the languages in the library and the selected one list catalog Bibles; reading the rest would be a lot of queries.
  const catalogBibles = useCatalogBibles([lang, ...libraryCodes], enabled, catalogVersion);
  if (!enabled) return undefined;
  const choicesFor = (code: string) => bibleChoices(code, publications, catalogBibles[code] ?? []);
  return {
    choicesFor,
    current: pickBible(choicesFor(lang), bibles[lang] ?? null)?.symbol ?? null,
    progress: (c) => (c.entry ? downloads[taskKey(c.entry)] : undefined),
    onPick: (c) => {
      setLang(c.lang);
      chooseBible(c.lang, c.symbol);
      void (async () => {
        const place = view;
        const dir = c.dir ?? (c.entry ? await download(c.entry, c.lang, { quiet: true }) : null);
        if (!dir) return;
        try {
          const { books } = await api.publication(dir);
          switchTo("bible", { name: "publication", dir }, undefined, bibleStackFor(place, dir, books));
        } catch {
          switchTo("bible", { name: "publication", dir });
        }
      })();
    },
  };
}

/** Top bar of a view: title (with optional subtitle) and actions on the right. */
export function AppBar({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children?: ReactNode;
}) {
  const { push, lang, setLang, publications, section } = useApp();
  const libraryCodes = [...new Set(publications.map((p) => p.langCode).filter((c): c is string => !!c))];
  const translations = useBibleTranslations(section === "bible");
  return (
    <header className="flex h-12 shrink-0 items-center gap-1 border-b border-line bg-surface pl-4 pr-2">
      <div className="min-w-0 flex-1 leading-tight">
        <h1 className="truncate text-sm font-semibold">{title}</h1>
        {subtitle && <div className="truncate text-[13px] text-muted">{subtitle}</div>}
      </div>
      {children}
      <BarButton label="Search and download" onClick={() => push({ name: "online" })}>
        <Search size={18} strokeWidth={1.6} />
      </BarButton>
      <LanguageMenu value={lang} onChange={setLang} libraryCodes={libraryCodes} translations={translations} />
      <MoreMenu />
    </header>
  );
}

/** Section heading used across views ("Favorites", "What's New", …). `quiet` drops the top margin, for a heading that opens a page. */
export function SectionTitle({
  children,
  aside,
  variant = "default",
}: {
  children: ReactNode;
  aside?: ReactNode;
  variant?: "default" | "quiet";
}) {
  const quiet = variant === "quiet";
  return (
    <div className={cn("flex items-baseline justify-between", quiet ? "mb-3" : "mb-3 mt-10")}>
      <h2 className="text-sm font-semibold">
        {typeof children === "string" ? t(children) : children}
      </h2>
      {aside && <span className="text-[13px] text-muted">{aside}</span>}
    </div>
  );
}

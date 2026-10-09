import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ask, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
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
  X,
} from "lucide-react";
import { api, type CatalogEntry, type Language, type Progress, type PubCard, type Target, type UpdateInfo } from "@/lib/api";
import type { MediaRef } from "@/lib/api";
import type { PlayerItem } from "@/lib/player";
import { linkedItem } from "@/lib/recording";
import { DISPLAY_CLOSED, DISPLAY_READY, remember, sendToDisplay, type DisplayMessage } from "@/lib/display";
import { currentView, initialNav, navReducer, type View } from "@/lib/nav";
import {
  inLanguage,
  loadFavorites,
  loadFontScale,
  loadSecondDisplay,
  loadUpdateCheck,
  saveSecondDisplay,
  saveUpdateCheck,
  loadLang,
  loadUiLang,
  loadTheme,
  prefersDark,
  saveFontScale,
  toggleFavorite as toggleStoredFavorite,
  saveLang,
  saveUiLang,
  saveTheme,
  type Theme,
} from "@/lib/settings";
import { cn } from "@/lib/utils";
import { isoDate } from "@/lib/dates";
import { resolveUiLang, setUiLang, t, type UiLangSetting } from "@/lib/i18n";
import { LanguageMenu } from "@/components/language-menu";

interface Toast {
  id: number;
  text: string;
  action?: { label: string; run: () => void };
}

interface AppContextValue {
  view: View;
  canGoBack: boolean;
  push: (view: View) => void;
  replace: (view: View) => void;
  root: (view: View) => void;
  back: () => void;
  openTarget: (target: Target, note?: boolean) => void;
  publications: PubCard[];
  refreshPublications: () => Promise<void>;
  /** Delete a downloaded publication from the library. */
  removePublication: (dir: string, title: string) => Promise<void>;
  toast: (text: string, action?: Toast["action"]) => void;
  theme: Theme;
  setTheme: (t: Theme) => void;
  dark: boolean;
  fontScale: number;
  setFontScale: (n: number) => void;
  importFiles: () => Promise<void>;
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
  /** Download and import a catalog publication; resolves to its directory. `lang` defaults to the publication language. */
  download: (entry: CatalogEntry, lang?: string) => Promise<string | null>;
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
  /** Favorite keys (`symbol_meps[_issue]`, the library directory names). */
  favorites: string[];
  toggleFavorite: (key: string) => void;
  createBackup: () => Promise<void>;
  restoreBackup: () => Promise<void>;
  /** The recording being played; it keeps playing while the user navigates. */
  player: { id: number; item: PlayerItem } | null;
  playItem: (item: PlayerItem) => void;
  /** Look up a recording that a publication links to and play it. */
  playRecording: (media: MediaRef) => Promise<void>;
  closePlayer: () => void;
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

export const taskKey = (e: CatalogEntry) => `download:${e.item.symbol}:${e.item.issue_tag}`;

function useSystemDark(): boolean {
  const query = "(prefers-color-scheme: dark)";
  const [dark, setDark] = useState(() => window.matchMedia?.(query).matches ?? false);
  useEffect(() => {
    const mq = window.matchMedia?.(query);
    if (!mq) return;
    const on = (e: MediaQueryListEvent) => setDark(e.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return dark;
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [nav, dispatch] = useReducer(navReducer, initialNav);
  const [publications, setPublications] = useState<PubCard[]>([]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [theme, setThemeState] = useState<Theme>(loadTheme);
  const [fontScale, setFontScaleState] = useState(loadFontScale);
  const [lang, setLangState] = useState(loadLang);
  const [uiLang, setUiLangState] = useState<UiLangSetting>(loadUiLang);
  const [languages, setLanguages] = useState<Language[]>([]);
  const [downloads, setDownloads] = useState<Record<string, Progress>>({});
  const [catalogVersion, setCatalogVersion] = useState(0);
  const [userVersion, setUserVersion] = useState(0);
  const [favorites, setFavorites] = useState(loadFavorites);
  const toggleFavorite = useCallback((key: string) => setFavorites(toggleStoredFavorite(key)), []);
  const [player, setPlayer] = useState<AppContextValue["player"]>(null);
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

  const toast = useCallback((text: string, action?: Toast["action"]) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, action }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), action ? 9000 : 5000);
  }, []);

  const refreshPublications = useCallback(async () => {
    try {
      setPublications(await api.listPublications());
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
    async (entry: CatalogEntry, lang?: string) => {
      const key = taskKey(entry);
      setDownloads((d) => ({ ...d, [key]: { task: key, done: 0, total: entry.item.size } }));
      try {
        const dir = await api.downloadPublication(entry.item, lang ?? langRef.current);
        toast(t("Imported {title}", { title: entry.item.issue_title || entry.item.title }));
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

  const restoreBackup = useCallback(async () => {
    const picked = await openDialog({ multiple: false, filters: [{ name: "Backup", extensions: ["jwlibrary"] }] });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const ok = await ask(t("Restoring replaces all highlights, notes, tags and bookmarks with those in the backup."), {
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
  }, [toast]);

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

  const value = useMemo<AppContextValue>(
    () => ({
      view: currentView(nav),
      canGoBack: nav.stack.length > 1,
      push: (view) => dispatch({ type: "push", view }),
      replace: (view) => dispatch({ type: "replace", view }),
      root: (view) => dispatch({ type: "root", view }),
      back: () => dispatch({ type: "back" }),
      openTarget: (target, note) => dispatch({ type: "push", view: { name: "reader", target, note } }),
      publications,
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
      importFiles,
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
      favorites,
      toggleFavorite,
      createBackup,
      restoreBackup,
      player,
      playItem,
      playRecording,
      closePlayer,
      display: { enabled: displayOn, setEnabled: setSecondDisplay, send: sendDisplay },
      viewer,
      showImage,
      closeImage,
    }),
    [
      nav,
      publications,
      refreshPublications,
      removePublication,
      toast,
      theme,
      dark,
      fontScale,
      importFiles,
      lang,
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
      favorites,
      toggleFavorite,
      createBackup,
      restoreBackup,
      player,
      playItem,
      playRecording,
      closePlayer,
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
      <div className="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2">
        {toasts.map((item) => (
          <div
            key={item.id}
            className="pointer-events-auto flex max-w-xl items-center gap-3 bg-[#2b2b2b] px-4 py-2.5 text-sm text-white shadow-lg"
          >
            <span>{item.text}</span>
            {item.action && (
              <button className="font-semibold text-[#e0b24f] hover:underline" onClick={item.action.run}>
                {item.action.label}
              </button>
            )}
            <button
              aria-label={t("Dismiss")}
              className="opacity-60 hover:opacity-100"
              onClick={() => setToasts((all) => all.filter((x) => x.id !== item.id))}
            >
              <X size={14} />
            </button>
          </div>
        ))}
      </div>
    </AppContext.Provider>
  );
}

/** Purple strip across the top with the back arrow, like a window title. */
export function TitleStrip() {
  const { canGoBack, back } = useApp();
  return (
    <div data-tauri-drag-region className="flex h-7 shrink-0 items-center gap-3 bg-brand px-2 text-xs text-brand-fg">
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
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  expanded: boolean;
  children: ReactNode;
}) {
  return (
    <button
      title={t(label)}
      aria-label={t(label)}
      onClick={onClick}
      className={cn(
        "relative flex h-14 w-full items-center gap-4 px-[13px] text-fg/75 hover:bg-white/5 hover:text-fg",
        active && "text-accent",
      )}
    >
      {active && <span className="absolute inset-y-3 left-0 w-[3px] bg-accent" />}
      <span className="flex w-6 justify-center">{children}</span>
      {expanded && <span className="text-sm">{t(label)}</span>}
    </button>
  );
}

/** Icon rail on the left; the menu button expands labels. */
export function Rail() {
  const { view, root, publications, toast, lang } = useApp();
  const [expanded, setExpanded] = useState(false);
  const bibles = inLanguage(publications, lang).filter((p) => p.isBible);
  const bible = bibles.find((p) => p.symbol === "nwtsty") ?? bibles[0];
  const inBible =
    (view.name === "publication" || view.name === "chapters") && bible !== undefined && view.dir === bible.dir;
  const go = (v: View) => {
    setExpanded(false);
    root(v);
  };
  const icon = { size: 22, strokeWidth: 1.4 };
  return (
    <nav className={cn("flex shrink-0 flex-col bg-rail transition-[width]", expanded ? "w-52" : "w-[50px]")}>
      <RailButton label="Menu" active={false} expanded={expanded} onClick={() => setExpanded((e) => !e)}>
        <Menu {...icon} />
      </RailButton>
      <RailButton label="Home" active={view.name === "home"} expanded={expanded} onClick={() => go({ name: "home" })}>
        <Home {...icon} />
      </RailButton>
      <RailButton
        label="Bible"
        active={inBible || (view.name === "reader" && "chapter" in view.target.kind)}
        expanded={expanded}
        onClick={() =>
          bible ? go({ name: "publication", dir: bible.dir }) : toast(t("No Bible in this language in your library yet"))
        }
      >
        <BookOpen {...icon} />
      </RailButton>
      <RailButton
        label="Library"
        active={view.name === "library" || view.name === "category" || view.name === "media"}
        expanded={expanded}
        onClick={() => go({ name: "library" })}
      >
        <LibraryBig {...icon} />
      </RailButton>
      <RailButton
        label="Meetings"
        active={view.name === "meetings"}
        expanded={expanded}
        onClick={() => go({ name: "meetings" })}
      >
        <Users {...icon} />
      </RailButton>
      <RailButton
        label="Personal Study"
        active={view.name === "personal"}
        expanded={expanded}
        onClick={() => go({ name: "personal" })}
      >
        <Gem {...icon} />
      </RailButton>
      <div className="flex-1" />
      <RailButton
        label="Settings"
        active={view.name === "settings"}
        expanded={expanded}
        onClick={() => go({ name: "settings" })}
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
      className="flex h-10 w-11 items-center justify-center text-fg/85 hover:bg-black/5 hover:text-fg disabled:opacity-40 dark:hover:bg-white/10"
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
      className="flex w-full items-center gap-3 px-4 py-2.5 text-left text-sm hover:bg-black/5 dark:hover:bg-white/10"
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
        <Ellipsis size={22} strokeWidth={1.5} />
      </BarButton>
      {open && (
        <div className="absolute right-0 top-11 z-40 w-56 bg-bar py-1 shadow-xl ring-1 ring-line">
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
  const { push, lang, setLang, publications } = useApp();
  const libraryCodes = [...new Set(publications.map((p) => p.langCode).filter((c): c is string => !!c))];
  return (
    <header className="flex h-[52px] shrink-0 items-center gap-1 bg-bar pl-4 pr-2">
      <div className="min-w-0 flex-1 leading-tight">
        <h1 className="truncate text-[0.95rem] font-semibold">{title}</h1>
        {subtitle && <div className="truncate text-[0.85rem] text-fg/80">{subtitle}</div>}
      </div>
      {children}
      <BarButton label="Search and download" onClick={() => push({ name: "online" })}>
        <Search size={21} strokeWidth={1.5} />
      </BarButton>
      <LanguageMenu value={lang} onChange={setLang} libraryCodes={libraryCodes} />
      <MoreMenu />
    </header>
  );
}

/** Section heading used across views ("Favorites", "What's New", …). */
export function SectionTitle({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="mb-3 mt-8 flex items-baseline justify-between">
      <h2 className="text-[1.35rem] font-semibold">{typeof children === "string" ? t(children) : children}</h2>
      {aside && <span className="text-sm text-accent">{aside}</span>}
    </div>
  );
}

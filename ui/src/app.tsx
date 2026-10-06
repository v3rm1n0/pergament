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
import { api, type CatalogEntry, type Language, type Progress, type PubCard, type Target } from "@/lib/api";
import { currentView, initialNav, navReducer, type View } from "@/lib/nav";
import {
  inLanguage,
  loadFontScale,
  loadLang,
  loadTheme,
  prefersDark,
  saveFontScale,
  saveLang,
  saveTheme,
  type Theme,
} from "@/lib/settings";
import { cn } from "@/lib/utils";
import { isoDate } from "@/lib/dates";
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
  /** Name of a language in itself, e.g. "Deutsch" for `X`. */
  languageName: (code: string) => string;
  /** Running downloads by `taskKey`. */
  downloads: Record<string, Progress>;
  /** Download and import a catalog publication; resolves to its directory. */
  download: (entry: CatalogEntry) => Promise<string | null>;
  /** Bumped whenever the catalog was (re)loaded or the library changed. */
  catalogVersion: number;
  loadCatalog: () => Promise<void>;
  /** Bumped when user data was replaced by a restored backup. */
  userVersion: number;
  createBackup: () => Promise<void>;
  restoreBackup: () => Promise<void>;
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
  const [languages, setLanguages] = useState<Language[]>([]);
  const [downloads, setDownloads] = useState<Record<string, Progress>>({});
  const [catalogVersion, setCatalogVersion] = useState(0);
  const [userVersion, setUserVersion] = useState(0);
  const systemDark = useSystemDark();
  const dark = prefersDark(theme, systemDark);

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
      toast(`Cannot read library: ${e}`);
    }
  }, [toast]);

  useEffect(() => {
    void refreshPublications();
  }, [refreshPublications]);

  const importFiles = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      directory: false,
      filters: [{ name: "JW publications", extensions: ["jwpub"] }],
    });
    const paths = picked == null ? [] : Array.isArray(picked) ? picked : [picked];
    if (paths.length === 0) return;
    toast(`Importing ${paths.length} file(s)…`);
    try {
      for (const r of await api.importFiles(paths)) {
        toast(r.error ? `Import failed: ${r.error}` : `Imported ${r.title}`);
      }
    } catch (e) {
      toast(`Import failed: ${e}`);
    }
    await refreshPublications();
    setCatalogVersion((v) => v + 1);
  }, [refreshPublications, toast]);

  const langRef = useRef(lang);
  langRef.current = lang;
  const download = useCallback(
    async (entry: CatalogEntry) => {
      const key = taskKey(entry);
      setDownloads((d) => ({ ...d, [key]: { task: key, done: 0, total: entry.item.size } }));
      try {
        const dir = await api.downloadPublication(entry.item, langRef.current);
        toast(`Imported ${entry.item.issue_title || entry.item.title}`);
        await refreshPublications();
        setCatalogVersion((v) => v + 1);
        return dir;
      } catch (e) {
        toast(`Download failed: ${e}`);
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

  const loadCatalog = useCallback(async () => {
    try {
      await api.loadCatalog();
      setCatalogVersion((v) => v + 1);
    } catch (e) {
      toast(`Catalog: ${e}`);
    }
  }, [toast]);

  const createBackup = useCallback(async () => {
    const path = await saveDialog({
      defaultPath: `UserdataBackup_${isoDate(new Date())}_Pergament.jwlibrary`,
      filters: [{ name: "Backup", extensions: ["jwlibrary"] }],
    });
    if (!path) return;
    try {
      await api.exportBackup(path);
      toast("Backup created");
    } catch (e) {
      toast(`Backup failed: ${e}`);
    }
  }, [toast]);

  const restoreBackup = useCallback(async () => {
    const picked = await openDialog({ multiple: false, filters: [{ name: "Backup", extensions: ["jwlibrary"] }] });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    const ok = await ask("Restoring replaces all highlights, notes, tags and bookmarks with those in the backup.", {
      title: "Restore backup?",
      kind: "warning",
      okLabel: "Restore",
    });
    if (!ok) return;
    try {
      const s = await api.restoreBackup(path);
      toast(`Restored ${s.marks} highlights and ${s.notes} notes${s.device ? ` from ${s.device}` : ""}`);
      setUserVersion((v) => v + 1);
    } catch (e) {
      toast(`Restore failed: ${e}`);
    }
  }, [toast]);

  const names = useMemo(() => new Map(languages.map((l) => [l.code, l.vernacular])), [languages]);

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
      languageName: (code) => names.get(code) ?? code,
      downloads,
      download,
      catalogVersion,
      loadCatalog,
      userVersion,
      createBackup,
      restoreBackup,
    }),
    [
      nav,
      publications,
      refreshPublications,
      toast,
      theme,
      dark,
      fontScale,
      importFiles,
      lang,
      names,
      downloads,
      download,
      catalogVersion,
      loadCatalog,
      userVersion,
      createBackup,
      restoreBackup,
    ],
  );

  return (
    <AppContext.Provider value={value}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            className="pointer-events-auto flex max-w-xl items-center gap-3 bg-[#2b2b2b] px-4 py-2.5 text-sm text-white shadow-lg"
          >
            <span>{t.text}</span>
            {t.action && (
              <button className="font-semibold text-[#e0b24f] hover:underline" onClick={t.action.run}>
                {t.action.label}
              </button>
            )}
            <button
              aria-label="Dismiss"
              className="opacity-60 hover:opacity-100"
              onClick={() => setToasts((all) => all.filter((x) => x.id !== t.id))}
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
        aria-label="Back"
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
      title={label}
      aria-label={label}
      onClick={onClick}
      className={cn(
        "relative flex h-14 w-full items-center gap-4 px-[13px] text-fg/75 hover:bg-white/5 hover:text-fg",
        active && "text-accent",
      )}
    >
      {active && <span className="absolute inset-y-3 left-0 w-[3px] bg-accent" />}
      <span className="flex w-6 justify-center">{children}</span>
      {expanded && <span className="text-sm">{label}</span>}
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
          bible ? go({ name: "publication", dir: bible.dir }) : toast("No Bible in this language in your library yet")
        }
      >
        <BookOpen {...icon} />
      </RailButton>
      <RailButton
        label="Library"
        active={view.name === "library" || view.name === "category"}
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
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={onClick}
      className="flex h-10 w-11 items-center justify-center text-fg/85 hover:bg-black/5 hover:text-fg dark:hover:bg-white/10"
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
      {label}
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
      <h2 className="text-[1.35rem] font-semibold">{children}</h2>
      {aside && <span className="text-sm text-accent">{aside}</span>}
    </div>
  );
}

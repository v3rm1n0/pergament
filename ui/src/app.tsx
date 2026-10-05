import { createContext, useCallback, useContext, useEffect, useMemo, useReducer, useState } from "react";
import type { ReactNode } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  BookOpen,
  CloudDownload,
  FolderInput,
  Home,
  Moon,
  Settings,
  Sun,
  X,
} from "lucide-react";
import { api, type PubCard, type Target } from "@/lib/api";
import { currentView, initialNav, navReducer, type View } from "@/lib/nav";
import { loadFontScale, loadTheme, prefersDark, saveFontScale, saveTheme, type Theme } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";

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
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp outside AppProvider");
  return ctx;
}

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
  const systemDark = useSystemDark();
  const dark = prefersDark(theme, systemDark);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
  }, [dark]);
  useEffect(() => {
    document.documentElement.style.setProperty("--font-scale", String(fontScale));
  }, [fontScale]);

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
  }, [refreshPublications, toast]);

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
    }),
    [nav, publications, refreshPublications, toast, theme, dark, fontScale, importFiles],
  );

  return (
    <AppContext.Provider value={value}>
      {children}
      <div className="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2">
        {toasts.map((t) => (
          <div
            key={t.id}
            className="pointer-events-auto flex max-w-xl items-center gap-3 rounded bg-[#2b2b30] px-4 py-2.5 text-sm text-white shadow-lg"
          >
            <span>{t.text}</span>
            {t.action && (
              <button className="font-semibold text-[#c9b6f2] hover:underline" onClick={t.action.run}>
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

/** Purple strip across the top, like a branded window title. */
export function TitleStrip() {
  return (
    <div data-tauri-drag-region className="flex h-8 shrink-0 items-center bg-brand px-4 text-[13px] text-brand-fg">
      jwlinux
    </div>
  );
}

function RailButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      title={label}
      aria-label={label}
      onClick={onClick}
      className={cn(
        "relative flex h-16 w-full items-center justify-center text-fg/70 hover:bg-black/5 hover:text-fg dark:hover:bg-white/5",
        active && "text-accent",
      )}
    >
      {active && <span className="absolute inset-y-3 left-0 w-1 rounded-r bg-accent" />}
      {children}
    </button>
  );
}

/** Icon rail on the left. */
export function Rail() {
  const { view, root, publications, toast } = useApp();
  const bible = publications.find((p) => p.isBible && p.symbol === "nwtsty") ?? publications.find((p) => p.isBible);
  const inBible =
    (view.name === "publication" || view.name === "chapters") && bible !== undefined && view.dir === bible.dir;
  return (
    <nav className="flex w-16 shrink-0 flex-col border-r border-line bg-rail">
      <RailButton label="Home" active={view.name === "home"} onClick={() => root({ name: "home" })}>
        <Home size={26} strokeWidth={1.5} />
      </RailButton>
      <RailButton
        label="Bible"
        active={inBible}
        onClick={() =>
          bible ? root({ name: "publication", dir: bible.dir }) : toast("No Bible in the library yet")
        }
      >
        <BookOpen size={26} strokeWidth={1.5} />
      </RailButton>
      <RailButton label="Search online" active={view.name === "online"} onClick={() => root({ name: "online" })}>
        <CloudDownload size={26} strokeWidth={1.5} />
      </RailButton>
      <div className="flex-1" />
      <RailButton label="Settings" active={view.name === "settings"} onClick={() => root({ name: "settings" })}>
        <Settings size={24} strokeWidth={1.5} />
      </RailButton>
    </nav>
  );
}

/** Top bar of a view: back button, title and actions. */
export function AppBar({ title, children }: { title: string; children?: ReactNode }) {
  const { canGoBack, back, push, importFiles, dark, setTheme } = useApp();
  return (
    <header className="flex h-16 shrink-0 items-center gap-2 border-b border-line bg-bar px-3">
      {canGoBack ? (
        <Button variant="ghost" size="icon" aria-label="Back" onClick={back}>
          <ArrowLeft size={22} />
        </Button>
      ) : (
        <span className="w-2" />
      )}
      <h1 className="min-w-0 flex-1 truncate text-[1.15rem] font-semibold">{title}</h1>
      {children}
      <Button variant="ghost" size="icon" title="Import .jwpub" aria-label="Import" onClick={() => void importFiles()}>
        <FolderInput size={22} strokeWidth={1.6} />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        title="Search and download"
        aria-label="Search online"
        onClick={() => push({ name: "online" })}
      >
        <CloudDownload size={22} strokeWidth={1.6} />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        title={dark ? "Light mode" : "Dark mode"}
        aria-label="Toggle theme"
        onClick={() => setTheme(dark ? "light" : "dark")}
      >
        {dark ? <Sun size={22} strokeWidth={1.6} /> : <Moon size={22} strokeWidth={1.6} />}
      </Button>
    </header>
  );
}

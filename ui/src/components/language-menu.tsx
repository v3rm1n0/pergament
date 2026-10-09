import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Languages } from "lucide-react";
import { api, type Language } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { t } from "@/lib/i18n";

// The list is the same for the whole session; load it once.
let cache: Promise<Language[]> | null = null;
export function loadLanguages(): Promise<Language[]> {
  cache ??= api.languages().catch((e) => {
    cache = null;
    throw e;
  });
  return cache;
}

/** Language button for the top-right corner of the app bar. */
export function LanguageMenu({
  value,
  onChange,
  libraryCodes,
}: {
  value: string;
  onChange: (code: string) => void;
  libraryCodes: string[];
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [languages, setLanguages] = useState<Language[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    loadLanguages()
      .then(setLanguages)
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const byCode = useMemo(() => new Map(languages?.map((l) => [l.code, l]) ?? []), [languages]);
  const current = byCode.get(value);
  const q = query.trim().toLowerCase();
  const matches = (l: Language) =>
    !q || l.code.toLowerCase() === q || l.name.toLowerCase().includes(q) || l.vernacular.toLowerCase().includes(q);

  const inLibrary = libraryCodes.map((c) => byCode.get(c) ?? { code: c, name: c, vernacular: c, rtl: false, signLanguage: false });
  const all = (languages ?? []).filter((l) => !libraryCodes.includes(l.code));

  const row = (l: Language) => (
    <button
      key={l.code}
      onClick={() => {
        onChange(l.code);
        setOpen(false);
        setQuery("");
      }}
      className={cn(
        "flex w-full items-center gap-3 px-4 py-2 text-left hover:bg-bar",
        l.code === value && "text-accent",
      )}
    >
      <span className="min-w-0 flex-1">
        <span className="block truncate font-medium" dir={l.rtl ? "rtl" : undefined}>
          {l.vernacular}
        </span>
        <span className="block truncate text-xs text-muted">
          {l.name}
        </span>
      </span>
      {l.code === value && <Check size={16} />}
    </button>
  );

  return (
    <div ref={ref} className="relative">
      <button
        title={t("Language: {name}", { name: current?.vernacular ?? value })}
        aria-label={t("Language")}
        onClick={() => setOpen((o) => !o)}
        className="flex h-10 items-center gap-1.5 rounded px-2.5 hover:bg-black/5 dark:hover:bg-white/10"
      >
        <Languages size={22} strokeWidth={1.6} />
        <span className="text-sm font-semibold">{value}</span>
      </button>
      {open && (
        <div className="absolute right-0 top-12 z-40 flex max-h-[70vh] w-80 flex-col overflow-hidden rounded bg-surface shadow-xl ring-1 ring-line">
          <div className="border-b border-line p-3">
            <Input
              autoFocus
              placeholder={t("Search languages")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className="w-full"
            />
          </div>
          <div className="flex-1 overflow-y-auto py-1">
            {inLibrary.filter(matches).length > 0 && (
              <>
                <div className="px-4 pb-1 pt-2 text-xs font-semibold uppercase tracking-wider text-muted">
                  {t("In your library")}
                </div>
                {inLibrary.filter(matches).map(row)}
              </>
            )}
            <div className="px-4 pb-1 pt-3 text-xs font-semibold uppercase tracking-wider text-muted">
              {t("All languages")}
            </div>
            {error && <p className="px-4 py-2 text-sm text-muted">{t("Language list unavailable: {error}", { error })}</p>}
            {!languages && !error && <p className="px-4 py-2 text-sm text-muted">{t("Loading…")}</p>}
            {all.filter(matches).map(row)}
          </div>
        </div>
      )}
    </div>
  );
}

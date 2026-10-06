import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  BookOpen,
  BookText,
  CalendarClock,
  Castle,
  FileText,
  GraduationCap,
  Layers,
  ListChecks,
  Mic,
  Newspaper,
  NotebookTabs,
  ScrollText,
  Sparkles,
  Users,
} from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, type CatalogEntry, type Category } from "@/lib/api";
import { inLanguage } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { CatalogPrompt, GridCard } from "@/components/catalog";
import { HomeLibraryGrid } from "./downloaded";
import { importedOnly, mergeCategories, sectionsFor } from "@/lib/types";
import { t } from "@/lib/i18n";

/** Icons per category id (see CATEGORIES in src/catalog.rs). */
const ICONS: Record<number, LucideIcon> = {
  1: BookOpen,
  2: BookText,
  4: NotebookTabs,
  6: Layers,
  7: Newspaper,
  10: ScrollText,
  13: Sparkles,
  14: Castle,
  17: ListChecks,
  22: FileText,
  30: Users,
  31: CalendarClock,
  [-2]: GraduationCap,
  [-3]: Mic,
};

function Tabs({ value, onChange }: { value: string; onChange: (v: "publications" | "downloaded") => void }) {
  const tab = (id: "publications" | "downloaded", label: string) => (
    <button
      onClick={() => onChange(id)}
      className={cn(
        "relative px-2 pb-2.5 pt-3 text-[0.85rem] font-medium uppercase tracking-wide text-fg/85",
        value === id && "text-accent after:absolute after:inset-x-2 after:bottom-0 after:h-[2px] after:bg-accent",
      )}
    >
      {label}
    </button>
  );
  return (
    <div className="flex gap-3 bg-bar px-3">
      {tab("publications", t("Publications"))}
      {tab("downloaded", t("Downloaded"))}
    </div>
  );
}

export function LibraryView({ tab = "publications" }: { tab?: "publications" | "downloaded" }) {
  const { lang, languageName, replace, push, catalogVersion, toast, publications } = useApp();
  const [categories, setCategories] = useState<Category[] | null | undefined>(undefined);
  const local = inLanguage(publications, lang);
  const shownCategories = mergeCategories(categories ?? [], local);

  useEffect(() => {
    let live = true;
    api
      .categories(lang)
      .then((c) => live && setCategories(c))
      .catch((e) => {
        toast(String(e));
        if (live) setCategories(null);
      });
    return () => {
      live = false;
    };
  }, [lang, catalogVersion, toast]);

  return (
    <>
      <AppBar title={t("Library")} subtitle={languageName(lang)} />
      <Tabs value={tab} onChange={(t) => replace({ name: "library", tab: t })} />
      <div className="flex-1 overflow-y-auto px-5 py-5">
        {tab === "downloaded" ? (
          <HomeLibraryGrid pubs={local} />
        ) : categories === null && shownCategories.length === 0 ? (
          <CatalogPrompt />
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-1">
            {shownCategories.map((c) => {
              const Icon = ICONS[c.id] ?? FileText;
              return (
                <button
                  key={c.id}
                  onClick={() => push({ name: "category", id: c.id, title: t(c.name) })}
                  className="flex h-[86px] items-center gap-5 bg-tile px-6 text-left text-[0.95rem] hover:brightness-125"
                >
                  <Icon size={36} strokeWidth={1.1} className="shrink-0" />
                  <span className="min-w-0 hyphens-auto break-words">{t(c.name)}</span>
                </button>
              );
            })}
          </div>
        )}
      </div>
    </>
  );
}

export function CategoryView({ id, title }: { id: number; title: string }) {
  const { lang, languageName, catalogVersion, toast, publications } = useApp();
  const [items, setItems] = useState<CatalogEntry[] | null | undefined>(undefined);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    let live = true;
    api
      .category(lang, id)
      .then((c) => live && setItems(c))
      .catch((e) => toast(String(e)));
    return () => {
      live = false;
    };
  }, [lang, id, catalogVersion, toast]);

  const q = filter.trim().toLowerCase();
  const all = [...(items ?? []), ...importedOnly(items ?? [], inLanguage(publications, lang), id)];
  const shown = all.filter(
    (e) => !q || `${e.item.title} ${e.item.issue_title ?? ""} ${e.item.symbol}`.toLowerCase().includes(q),
  );
  return (
    <>
      <AppBar title={title} subtitle={languageName(lang)} />
      <div className="flex-1 overflow-y-auto px-5 py-5">
        {items === null && all.length === 0 ? (
          <CatalogPrompt />
        ) : (
          <>
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t("Filter")}
              className="mb-5 h-9 w-72 bg-tile px-3 text-sm outline-none placeholder:text-muted focus:ring-1 focus:ring-accent"
            />
            {sectionsFor(id, shown).map((s) => (
              <section key={s.title ?? ""} className="mb-8">
                {s.title && <h2 className="mb-3 text-[1.35rem] font-semibold">{s.title}</h2>}
                {s.parts.map((part) => (
                  <div key={part.title ?? ""} className="mb-6">
                    {part.title && <h3 className="mb-3 text-[1rem] font-medium text-muted">{part.title}</h3>}
                    <div className="grid grid-cols-[repeat(auto-fill,minmax(130px,1fr))] gap-x-3 gap-y-5">
                      {part.entries.map((e) => (
                        <GridCard key={`${e.item.symbol}-${e.item.issue_tag}`} entry={e} />
                      ))}
                    </div>
                  </div>
                ))}
              </section>
            ))}
          </>
        )}
      </div>
    </>
  );
}

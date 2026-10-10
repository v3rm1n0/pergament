import { useEffect, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  BookOpen,
  BookText,
  CalendarClock,
  Castle,
  CloudDownload,
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
import { AppBar, BarButton, taskKey, useApp } from "@/app";
import { api, type CatalogEntry, type Category, type PubCard } from "@/lib/api";
import { inLanguage } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { CatalogPrompt } from "@/components/catalog";
import { GridCard } from "@/components/cover";
import { HomeLibraryGrid } from "./downloaded";
import { importedOnly, mergeCategories, sectionsFor } from "@/lib/types";
import type { LibraryTab } from "@/lib/nav";
import { DownloadedRecordings, MediaTab } from "./media";
import { UpdatesTab } from "./updates";
import { useMediaDownloads } from "@/components/media-card";
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

function Tabs({ value, onChange }: { value: LibraryTab; onChange: (v: LibraryTab) => void }) {
  const { updates } = useApp();
  const tab = (id: LibraryTab, label: string, badge = 0) => (
    <button
      onClick={() => onChange(id)}
      aria-current={value === id ? "page" : undefined}
      className={cn(
        "shrink-0 whitespace-nowrap rounded-md px-3 py-1 text-[13px] font-medium text-muted hover:text-fg focus-visible:outline-2 focus-visible:outline-accent",
        value === id && "bg-bar text-fg",
      )}
    >
      {label}
      {badge > 0 && (
        <span className="ml-1.5 rounded-full bg-fg px-1.5 text-xs font-semibold tabular-nums text-surface">
          {badge}
        </span>
      )}
    </button>
  );
  return (
    <div className="flex gap-1 overflow-x-auto border-b border-line bg-surface px-4 py-2 min-[720px]:px-6">
      {tab("publications", t("Publications"))}
      {tab("video", t("Video"))}
      {tab("audio", t("Audio"))}
      {tab("downloaded", t("Downloaded"))}
      {tab("updates", t("Updates"), updates.items.length)}
    </div>
  );
}

/** Downloaded recordings and publications. */
function DownloadedTab({ pubs }: { pubs: PubCard[] }) {
  const { lang } = useApp();
  const store = useMediaDownloads();
  const mine = store.list.filter((m) => m.langCode === lang);
  return (
    <>
      <DownloadedRecordings items={mine} store={store} />
      {(pubs.length > 0 || mine.length === 0) && <HomeLibraryGrid pubs={pubs} />}
    </>
  );
}

export function LibraryView({ tab = "publications" }: { tab?: LibraryTab }) {
  const { lang, languageName, replace, push, catalogVersion, toast, publications, updates, downloads } = useApp();
  const updating = updates.items.some((u) => downloads[taskKey(u.entry)]);
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
      <AppBar title={t("Library")} subtitle={languageName(lang)}>
        {tab === "updates" && updates.items.length > 0 && (
          <BarButton label="Update all" disabled={updating} onClick={() => void updates.updateAll()}>
            <CloudDownload size={18} strokeWidth={1.6} />
          </BarButton>
        )}
      </AppBar>
      <Tabs value={tab} onChange={(t) => replace({ name: "library", tab: t })} />
      <div className="page flex-1 overflow-y-auto">
        {tab === "video" || tab === "audio" ? (
          <MediaTab kind={tab} />
        ) : tab === "updates" ? (
          <UpdatesTab />
        ) : tab === "downloaded" ? (
          <DownloadedTab pubs={local} />
        ) : categories === null && shownCategories.length === 0 ? (
          <CatalogPrompt />
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-2">
            {shownCategories.map((c) => {
              const Icon = ICONS[c.id] ?? FileText;
              return (
                <button
                  key={c.id}
                  onClick={() => push({ name: "category", id: c.id, title: t(c.name) })}
                  className="flex h-14 items-center gap-3 rounded-md border border-line px-4 text-left text-sm hover:bg-bar"
                >
                  <Icon size={20} strokeWidth={1.4} className="shrink-0 text-muted" />
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
      <div className="page flex-1 overflow-y-auto">
        {items === null && all.length === 0 ? (
          <CatalogPrompt />
        ) : (
          <>
            <Input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t("Filter")}
              aria-label={t("Filter")}
              className="mb-8 w-full max-w-72"
            />
            {sectionsFor(id, shown).map((s) => (
              <section key={s.title ?? ""} className="mb-10">
                {s.title && <h2 className="mb-3 text-sm font-semibold">{s.title}</h2>}
                {s.parts.map((part) => (
                  <div key={part.title ?? ""} className="mb-6">
                    {part.title && <h3 className="mb-3 text-[13px] font-medium text-muted">{part.title}</h3>}
                    <div className="grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-x-3 gap-y-4">
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

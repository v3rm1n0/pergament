import { MediaImg } from "@/components/media-img";
import { useState } from "react";
import type { ReactNode } from "react";
import { BookOpen, CloudDownload, FileText } from "lucide-react";
import { taskKey, useApp } from "@/app";
import type { CatalogEntry } from "@/lib/api";
import { ago } from "@/lib/dates";
import { cn } from "@/lib/utils";
import { TileMenu, entryTarget } from "@/components/tile-menu";
import { Progress } from "@/components/ui/progress";
import { t } from "@/lib/i18n";

export const mb = (n: number) => `${Math.max(1, Math.round(n / 1e6))} MB`;

/** Cover image of a catalog entry, with a neutral fallback. */
export function EntryImage({ entry, className }: { entry: CatalogEntry; className?: string }) {
  const [failed, setFailed] = useState(false);
  if (!entry.imageUrl || failed) {
    return (
      <div className={cn("flex items-center justify-center bg-tile text-muted", className)}>
        <FileText size={30} strokeWidth={1.2} />
      </div>
    );
  }
  return (
    <MediaImg
      src={entry.imageUrl}
      alt=""
      draggable={false}
      onError={() => setFailed(true)}
      className={cn("object-cover", className)}
    />
  );
}

/** Opens a downloaded entry or starts its download. */
export function useEntryAction() {
  const { push, download, downloads } = useApp();
  return {
    progress: (e: CatalogEntry) => downloads[taskKey(e)],
    activate: async (e: CatalogEntry) => {
      if (e.local) {
        push({ name: "publication", dir: e.local });
        return;
      }
      if (downloads[taskKey(e)]) return;
      const dir = await download(e);
      if (dir) push({ name: "publication", dir });
    },
  };
}

function ProgressOverlay({ entry }: { entry: CatalogEntry }) {
  const { progress } = useEntryAction();
  const p = progress(entry);
  if (!p) return null;
  return (
    <div className="absolute inset-x-1.5 bottom-1.5">
      <Progress value={p.total ? (p.done / p.total) * 100 : null} />
    </div>
  );
}

/** Caption box of a cover tile, always the same size so the tiles line up. */
export function CoverCaption({ children }: { children: ReactNode }) {
  return (
    <div className="mt-1 h-[1.8rem] overflow-hidden text-[0.68rem] leading-tight group-hover:underline">
      <div className="line-clamp-2">{children}</div>
    </div>
  );
}

/** Square cover tile with a caption, as in the Teaching Toolbox. */
export function CoverTile({ entry, label }: { entry: CatalogEntry; label?: string }) {
  const { activate } = useEntryAction();
  const { lang } = useApp();
  return (
    <div className="group relative w-[88px]">
      <button onClick={() => void activate(entry)} className="w-full text-left" title={entry.item.title}>
        <div className="relative">
          <EntryImage entry={entry} className="h-[88px] w-[88px]" />
          {!entry.local && (
            <CloudDownload
              size={18}
              strokeWidth={1.6}
              className="absolute bottom-1 right-1 text-white drop-shadow-[0_0_2px_rgba(0,0,0,0.9)]"
            />
          )}
          <ProgressOverlay entry={entry} />
        </div>
        <CoverCaption>{label ?? entry.item.short_title ?? entry.item.title}</CoverCaption>
      </button>
      <TileMenu target={entryTarget(entry, lang)} />
    </div>
  );
}

/** Wide card with thumbnail, category, title, language, age and size. */
export function EntryCard({ entry, language, footer }: { entry: CatalogEntry; language: string; footer?: ReactNode }) {
  const { activate } = useEntryAction();
  const { lang } = useApp();
  return (
    <div className="group relative h-[88px] w-[300px] shrink-0">
      <button
        onClick={() => void activate(entry)}
        className="relative flex h-full w-full bg-tile text-left hover:brightness-110"
      >
        <EntryImage entry={entry} className="h-[88px] w-[88px] shrink-0" />
        <div className="flex min-w-0 flex-1 flex-col px-3 py-2">
          <div className="truncate text-xs text-fg/70">{entry.category && t(entry.category)}</div>
          <div className="line-clamp-2 text-[0.9rem] leading-snug">{entry.item.issue_title || entry.item.title}</div>
          <div className="mt-auto flex items-end justify-between text-xs text-fg/70">
            <span>{footer ?? `${language} · ${ago(entry.item.cataloged_on)}`}</span>
            <span className="flex flex-col items-end">
              {entry.local ? <BookOpen size={17} strokeWidth={1.5} /> : <CloudDownload size={17} strokeWidth={1.5} />}
              {mb(entry.item.size)}
            </span>
          </div>
        </div>
        <ProgressOverlay entry={entry} />
      </button>
      <TileMenu target={entryTarget(entry, lang)} />
    </div>
  );
}

/** Large grid card for category pages. */
export function GridCard({ entry }: { entry: CatalogEntry }) {
  const { activate } = useEntryAction();
  const { lang } = useApp();
  return (
    <div className="group relative">
      <button onClick={() => void activate(entry)} className="w-full text-left" title={entry.item.title}>
        <div className="relative">
          <EntryImage entry={entry} className="aspect-square w-full" />
          {!entry.local && (
            <CloudDownload
              size={20}
              strokeWidth={1.6}
              className="absolute bottom-1.5 right-1.5 text-white drop-shadow-[0_0_2px_rgba(0,0,0,0.9)]"
            />
          )}
          <ProgressOverlay entry={entry} />
        </div>
        <div className="mt-1.5 line-clamp-2 text-[0.8rem] leading-snug group-hover:underline">
          {entry.item.issue_title || entry.item.short_title || entry.item.title}
        </div>
        {entry.item.size > 0 && <div className="text-[0.7rem] text-muted">{mb(entry.item.size)}</div>}
      </button>
      <TileMenu target={entryTarget(entry, lang)} />
    </div>
  );
}

/** Shown where catalog content would be when no catalog is cached yet. */
export function CatalogPrompt() {
  const { loadCatalog } = useApp();
  const [loading, setLoading] = useState(false);
  return (
    <div className="max-w-xl bg-tile px-5 py-4 text-sm">
      <p className="mb-3 text-fg/85">
        {t(
          "New publications, categories and meeting materials come from the public jw.org catalog (about 58 MB, checked for updates once a day). Nothing is downloaded until you ask.",
        )}
      </p>
      <button
        disabled={loading}
        onClick={async () => {
          setLoading(true);
          await loadCatalog();
          setLoading(false);
        }}
        className="bg-brand px-4 py-2 text-brand-fg hover:brightness-110 disabled:opacity-50"
      >
        {loading ? t("Loading catalog…") : t("Load catalog")}
      </button>
    </div>
  );
}

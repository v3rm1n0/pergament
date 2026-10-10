import type { ReactNode } from "react";
import { BookOpen, CloudDownload, Star } from "lucide-react";
import { useApp } from "@/app";
import type { CatalogEntry, PubCard } from "@/lib/api";
import { pubKey } from "@/lib/share";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";
import { EntryImage, mb, useEntryAction } from "@/components/catalog";
import { MediaImg } from "@/components/media-img";
import { Progress } from "@/components/ui/progress";
import { TileMenu, cardTarget, coverControl as control, entryTarget, type PubTarget } from "@/components/tile-menu";

function Cover({
  image,
  title,
  category,
  target,
  onActivate,
  favoriteKey,
  progress,
  action,
  meta,
  className,
}: {
  image: ReactNode;
  title: string;
  category?: string | null;
  target: PubTarget;
  onActivate: () => void;
  favoriteKey: string;
  /** Percent while downloading, null while the size is unknown, undefined when idle. */
  progress?: number | null;
  action?: ReactNode;
  /** Replaces the category line. */
  meta?: string | null;
  className?: string;
}) {
  const { favorites, toggleFavorite } = useApp();
  const favorite = favorites.includes(favoriteKey);
  const label = favorite ? t("Remove from favorites") : t("Add to favorites");
  return (
    <div className={cn("group relative w-[104px]", className)}>
      <button
        onClick={onActivate}
        title={target.title}
        className="block w-full rounded-md text-left focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
      >
        <div className="relative aspect-[4/5] w-full overflow-hidden rounded-md border border-line bg-tile">
          {image}
          {progress !== undefined && (
            <Progress value={progress} label={target.title} className="absolute inset-x-0 bottom-0 h-[3px] rounded-none" />
          )}
        </div>
        <div className="mt-1.5 line-clamp-2 text-[13px] font-medium leading-snug">{title}</div>
        {(meta ?? category) && <div className="truncate text-xs text-muted">{meta ?? category}</div>}
      </button>
      <div className="pointer-events-none absolute inset-x-0 top-0 aspect-[4/5] [&>*]:pointer-events-auto">
        <button
          aria-label={label}
          aria-pressed={favorite}
          title={label}
          onClick={() => toggleFavorite(favoriteKey)}
          className={cn(control, "absolute right-[38px] top-1.5")}
        >
          <Star size={14} strokeWidth={1.6} className={cn(favorite && "fill-accent text-accent")} />
        </button>
        <TileMenu target={target} />
        {action}
      </div>
    </div>
  );
}

/** Portrait cover of a catalog entry, the kind that is downloaded when it is not in the library yet. */
export function EntryCover({
  entry,
  label,
  meta,
  className,
}: {
  entry: CatalogEntry;
  label?: string;
  meta?: string | null;
  className?: string;
}) {
  const { lang } = useApp();
  const { activate, progress } = useEntryAction();
  const target = entryTarget(entry, lang);
  const p = progress(entry);
  const verb = entry.local ? t("Open {title}", { title: target.title }) : t("Download {title}", { title: target.title });
  return (
    <Cover
      image={<EntryImage entry={entry} className="h-full w-full" />}
      title={label ?? entry.item.short_title ?? entry.item.title}
      category={entry.category && t(entry.category)}
      target={target}
      meta={meta}
      className={className}
      favoriteKey={pubKey(target.symbol, target.mepsLanguage, target.issueTag)}
      onActivate={() => void activate(entry)}
      progress={p ? (p.total ? (p.done / p.total) * 100 : null) : undefined}
      action={
        <button
          aria-label={verb}
          title={verb}
          disabled={!!p}
          onClick={() => void activate(entry)}
          className={cn(control, "absolute bottom-1.5 right-1.5")}
        >
          {entry.local ? <BookOpen size={14} strokeWidth={1.6} /> : <CloudDownload size={14} strokeWidth={1.6} />}
        </button>
      }
    />
  );
}

/** Portrait cover of a downloaded publication. */
export function PubCover({ pub, meta, className }: { pub: PubCard; meta?: string; className?: string }) {
  const { push, lang, favorites } = useApp();
  const target = cardTarget(pub, lang);
  const key = pubKey(target.symbol, target.mepsLanguage, target.issueTag);
  return (
    <Cover
      image={
        pub.cover ? (
          <MediaImg src={pub.cover} alt="" className="h-full w-full object-cover" draggable={false} />
        ) : (
          <div className="flex h-full w-full items-center justify-center bg-tile text-muted">
            <BookOpen size={28} strokeWidth={1.2} />
          </div>
        )
      }
      title={pub.shortTitle ?? pub.title}
      meta={meta}
      className={className}
      target={target}
      // A favorite of a non-Latin symbol is stored under its directory name.
      favoriteKey={favorites.includes(pub.dir) && !favorites.includes(key) ? pub.dir : key}
      onActivate={() => push({ name: "publication", dir: pub.dir })}
    />
  );
}

/** Entry cover in a fluid grid: larger entries show their size under the title. */
export function GridCard({ entry }: { entry: CatalogEntry }) {
  return (
    <EntryCover
      entry={entry}
      label={entry.item.issue_title || entry.item.short_title || entry.item.title}
      meta={entry.item.size > 0 ? mb(entry.item.size) : null}
      className="w-full"
    />
  );
}

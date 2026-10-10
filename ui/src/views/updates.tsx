import { useEffect, useState } from "react";
import { taskKey, useApp } from "@/app";
import { api, type UpdateInfo } from "@/lib/api";
import { CatalogPrompt, EntryImage, mb } from "@/components/catalog";
import { TileMenu, entryTarget } from "@/components/tile-menu";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { t } from "@/lib/i18n";

function UpdateRow({ update }: { update: UpdateInfo }) {
  const { downloads, updates, lang } = useApp();
  const { item } = update.entry;
  const title = item.issue_title || item.title;
  const progress = downloads[taskKey(update.entry)];
  return (
    <li className="group relative flex items-center gap-3 border-b border-line px-1 py-2 hover:bg-bar">
      <EntryImage entry={update.entry} className="aspect-[4/5] w-9 shrink-0 rounded-[6px] border border-line" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm leading-normal">{title}</div>
        <div className="truncate text-[13px] leading-normal tabular-nums text-muted">
          {item.symbol} · {item.year} · {mb(item.size)}
        </div>
        {progress && (
          <Progress value={progress.total ? (progress.done / progress.total) * 100 : null} label={title} className="mt-1 h-0.5" />
        )}
      </div>
      <Button
        variant="outline"
        size="sm"
        className="h-7 px-2.5 text-[13px]"
        disabled={!!progress}
        onClick={() => void updates.update(update)}
        title={t("Update {title}", { title })}
      >
        {t("Update")}
      </Button>
      <span className="relative size-7 shrink-0">
        <TileMenu
          target={entryTarget(update.entry, lang)}
          className="right-0 top-0 size-7 border-transparent bg-transparent text-muted opacity-100 hover:text-fg"
        />
      </span>
    </li>
  );
}

/** Downloaded publications that have a newer version in the catalog. */
export function UpdatesTab() {
  const { updates, downloads, catalogVersion } = useApp();
  const [hasCatalog, setHasCatalog] = useState<boolean | null>(null);
  useEffect(() => {
    api.catalogCached().then(setHasCatalog, () => setHasCatalog(false));
  }, [catalogVersion]);

  if (hasCatalog === null) return null;
  if (!hasCatalog) return <CatalogPrompt />;

  const count = updates.items.length;
  const busy = updates.items.some((u) => downloads[taskKey(u.entry)]);
  return (
    <>
      <div className="mb-5 flex flex-wrap items-center gap-3">
        <p className="text-sm">
          {count === 0
            ? t("All downloaded publications are up to date.")
            : count === 1
              ? t("1 publication has an update.")
              : t("{count} publications have updates.", { count })}
        </p>
        {count > 1 && (
          <Button disabled={busy} onClick={() => void updates.updateAll()}>
            {t("Update all")}
          </Button>
        )}
        <Button variant="outline" disabled={updates.checking} onClick={() => void updates.check()}>
          {updates.checking ? t("Checking…") : t("Check now")}
        </Button>
      </div>
      {count > 0 && (
        <ul className="border-t border-line">
          {updates.items.map((u) => (
            <UpdateRow key={u.dir} update={u} />
          ))}
        </ul>
      )}
      <p className="mt-6 max-w-xl text-sm text-muted">
        {t(
          "Updating downloads the new version and replaces the old one. Your highlights, notes and bookmarks are kept.",
        )}
      </p>
    </>
  );
}

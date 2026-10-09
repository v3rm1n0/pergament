import { useEffect, useState } from "react";
import { CloudDownload } from "lucide-react";
import { taskKey, useApp } from "@/app";
import { api, type UpdateInfo } from "@/lib/api";
import { CatalogPrompt, EntryImage, mb } from "@/components/catalog";
import { Progress } from "@/components/ui/progress";
import { t } from "@/lib/i18n";

function UpdateRow({ update }: { update: UpdateInfo }) {
  const { downloads, updates } = useApp();
  const { item } = update.entry;
  const progress = downloads[taskKey(update.entry)];
  return (
    <div className="relative flex bg-tile">
      <EntryImage entry={update.entry} className="h-[88px] w-[88px] shrink-0" />
      <div className="flex min-w-0 flex-1 flex-col px-3 py-2">
        <div className="line-clamp-2 text-[0.9rem] leading-snug">{item.issue_title || item.title}</div>
        <div className="mt-auto text-xs text-fg/70">
          {item.symbol} · {item.year} · {mb(item.size)}
        </div>
      </div>
      <button
        disabled={!!progress}
        onClick={() => void updates.update(update)}
        className="flex w-20 shrink-0 flex-col items-center justify-center gap-1 text-xs hover:bg-bar disabled:opacity-50"
        title={t("Update {title}", { title: item.issue_title || item.title })}
      >
        <CloudDownload size={20} strokeWidth={1.5} />
        {t("Update")}
      </button>
      {progress && (
        <div className="absolute inset-x-1.5 bottom-1.5">
          <Progress value={progress.total ? (progress.done / progress.total) * 100 : null} />
        </div>
      )}
    </div>
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
        <p className="text-[0.95rem]">
          {count === 0
            ? t("All downloaded publications are up to date.")
            : count === 1
              ? t("1 publication has an update.")
              : t("{count} publications have updates.", { count })}
        </p>
        {count > 1 && (
          <button
            disabled={busy}
            onClick={() => void updates.updateAll()}
            className="bg-brand px-4 py-2 text-sm text-brand-fg hover:brightness-110 disabled:opacity-50"
          >
            {t("Update all")}
          </button>
        )}
        <button
          disabled={updates.checking}
          onClick={() => void updates.check()}
          className="bg-tile px-4 py-2 text-sm hover:brightness-110 disabled:opacity-50"
        >
          {updates.checking ? t("Checking…") : t("Check now")}
        </button>
      </div>
      {count > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(360px,1fr))] gap-1">
          {updates.items.map((u) => (
            <UpdateRow key={u.dir} update={u} />
          ))}
        </div>
      )}
      <p className="mt-6 max-w-xl text-sm text-muted">
        {t(
          "Updating downloads the new version and replaces the old one. Your highlights, notes and bookmarks are kept.",
        )}
      </p>
    </>
  );
}

import { MediaImg } from "@/components/media-img";
import { useState } from "react";
import { FileText } from "lucide-react";
import { taskKey, useApp } from "@/app";
import type { CatalogEntry } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
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

/** Opens a downloaded entry, or downloads it without opening it. */
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
      await download(e);
    },
  };
}

/** Shown where catalog content would be when no catalog is cached yet. */
export function CatalogPrompt() {
  const { loadCatalog } = useApp();
  const [loading, setLoading] = useState(false);
  return (
    <div className="max-w-xl text-sm leading-normal">
      <p className="mb-3 text-muted">
        {t(
          "New publications, categories and meeting materials come from the public jw.org catalog (about 58 MB, checked for updates once a day). Nothing is downloaded until you ask.",
        )}
      </p>
      <Button
        variant="outline"
        disabled={loading}
        onClick={async () => {
          setLoading(true);
          await loadCatalog();
          setLoading(false);
        }}
      >
        {loading ? t("Loading catalog…") : t("Load catalog")}
      </Button>
    </div>
  );
}

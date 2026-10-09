import { MediaImg } from "@/components/media-img";
import { useState } from "react";
import { BookOpen, CloudDownload, FolderInput } from "lucide-react";
import { useApp } from "@/app";
import { api, type PubCard } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { TileMenu, cardTarget } from "@/components/tile-menu";
import { groupByCategory } from "@/lib/types";
import { t } from "@/lib/i18n";

function Cover({ pub }: { pub: PubCard }) {
  const [failed, setFailed] = useState(false);
  if (!pub.cover || failed) {
    return (
      <div className="flex aspect-square w-full items-center justify-center bg-tile-1 text-tile-fg">
        <BookOpen size={40} strokeWidth={1.2} />
      </div>
    );
  }
  return (
    <MediaImg
      src={pub.cover}
      alt=""
      onError={() => setFailed(true)}
      className="aspect-square w-full object-cover"
      draggable={false}
    />
  );
}

function PubTile({ pub }: { pub: PubCard }) {
  const { push, refreshPublications, toast, lang } = useApp();
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="group relative flex flex-col overflow-hidden bg-surface shadow-sm ring-1 ring-line">
      <button className="text-left" onClick={() => push({ name: "publication", dir: pub.dir })}>
        <Cover pub={pub} />
        <div className="px-3 py-2.5">
          <div className="line-clamp-2 text-[0.95rem] font-semibold leading-snug">
            {pub.shortTitle ?? pub.title}
          </div>
          <div className="mt-1 text-xs text-muted">
            {pub.symbol} · {pub.year}
          </div>
        </div>
      </button>
      {confirming ? (
        <div className="absolute inset-x-1.5 top-1.5 flex gap-1 rounded bg-rail p-1 shadow ring-1 ring-line">
          <Button
            size="sm"
            className="min-w-0 flex-1 px-2"
            onClick={async () => {
              try {
                await api.removePublication(pub.dir);
                toast(t("Removed {title}", { title: pub.shortTitle ?? pub.title }));
              } catch (e) {
                toast(t("Remove failed: {error}", { error: String(e) }));
              }
              await refreshPublications();
            }}
          >
            {t("Remove")}
          </Button>
          <Button size="sm" variant="outline" className="min-w-0 flex-1 px-2" onClick={() => setConfirming(false)}>
            {t("Cancel")}
          </Button>
        </div>
      ) : (
        <TileMenu target={cardTarget(pub, lang)} onRemove={() => setConfirming(true)} />
      )}
    </div>
  );
}

function Section({ title, pubs }: { title: string; pubs: PubCard[] }) {
  if (pubs.length === 0) return null;
  return (
    <section className="mb-8">
      <h2 className="mb-3 text-[1.35rem] font-semibold">{title}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(170px,1fr))] gap-4">
        {pubs.map((p) => (
          <PubTile key={p.dir} pub={p} />
        ))}
      </div>
    </section>
  );
}

/** Downloaded publications grouped by their manifest publication type, newest first. */
export function HomeLibraryGrid({ pubs }: { pubs: PubCard[] }) {
  const { importFiles, push } = useApp();
  const groups = groupByCategory(
    [...pubs].sort((a, b) => b.year - a.year || b.issueTag.localeCompare(a.issueTag)),
  );
  if (pubs.length === 0) {
    return (
      <div className="mx-auto mt-20 max-w-md text-center">
        <BookOpen className="mx-auto mb-4 text-muted" size={56} strokeWidth={1.1} />
        <h2 className="mb-2 text-xl font-semibold">{t("Nothing downloaded in this language")}</h2>
        <p className="mb-6 text-muted">{t("Import .jwpub files, browse the publications, or pick another language.")}</p>
        <div className="flex justify-center gap-3">
          <Button onClick={() => void importFiles()}>
            <FolderInput size={18} /> {t("Import")}
          </Button>
          <Button variant="outline" onClick={() => push({ name: "library" })}>
            <CloudDownload size={18} /> {t("Publications")}
          </Button>
        </div>
      </div>
    );
  }
  return (
    <>
      {groups.map((g) => (
        <Section key={g.id} title={t(g.name)} pubs={g.pubs} />
      ))}
    </>
  );
}

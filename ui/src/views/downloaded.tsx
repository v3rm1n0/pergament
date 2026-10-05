import { useState } from "react";
import { BookOpen, CloudDownload, FolderInput, Trash2 } from "lucide-react";
import { useApp } from "@/app";
import { api, type PubCard } from "@/lib/api";
import { Button } from "@/components/ui/button";

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
    <img
      src={pub.cover}
      alt=""
      onError={() => setFailed(true)}
      className="aspect-square w-full object-cover"
      draggable={false}
    />
  );
}

function PubTile({ pub }: { pub: PubCard }) {
  const { push, refreshPublications, toast } = useApp();
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
      <div className="absolute right-1.5 top-1.5 opacity-0 transition-opacity group-hover:opacity-100">
        {confirming ? (
          <div className="flex gap-1 rounded bg-surface p-1 shadow">
            <Button
              size="sm"
              onClick={async () => {
                try {
                  await api.removePublication(pub.dir);
                  toast(`Removed ${pub.shortTitle ?? pub.title}`);
                } catch (e) {
                  toast(`Remove failed: ${e}`);
                }
                await refreshPublications();
              }}
            >
              Remove
            </Button>
            <Button size="sm" variant="outline" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </div>
        ) : (
          <Button
            size="icon"
            variant="outline"
            className="h-8 w-8"
            title="Remove from library"
            onClick={() => setConfirming(true)}
          >
            <Trash2 size={15} />
          </Button>
        )}
      </div>
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

/** Downloaded publications: Bibles first, then the rest, newest first. */
export function HomeLibraryGrid({ pubs }: { pubs: PubCard[] }) {
  const { importFiles, push } = useApp();
  const bibles = pubs.filter((p) => p.isBible);
  const others = pubs
    .filter((p) => !p.isBible)
    .sort((a, b) => b.year - a.year || b.issueTag.localeCompare(a.issueTag));
  if (pubs.length === 0) {
    return (
      <div className="mx-auto mt-20 max-w-md text-center">
        <BookOpen className="mx-auto mb-4 text-muted" size={56} strokeWidth={1.1} />
        <h2 className="mb-2 text-xl font-semibold">Nothing downloaded in this language</h2>
        <p className="mb-6 text-muted">Import .jwpub files, browse the publications, or pick another language.</p>
        <div className="flex justify-center gap-3">
          <Button onClick={() => void importFiles()}>
            <FolderInput size={18} /> Import
          </Button>
          <Button variant="outline" onClick={() => push({ name: "library" })}>
            <CloudDownload size={18} /> Publications
          </Button>
        </div>
      </div>
    );
  }
  return (
    <>
      <Section title="Bible" pubs={bibles} />
      <Section title="Publications" pubs={others} />
    </>
  );
}

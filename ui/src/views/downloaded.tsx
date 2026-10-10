import { CloudDownload, FolderInput } from "lucide-react";
import { useApp } from "@/app";
import type { PubCard } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { PubCover } from "@/components/cover";
import { groupByCategory } from "@/lib/types";
import { t } from "@/lib/i18n";

function Section({ title, pubs }: { title: string; pubs: PubCard[] }) {
  if (pubs.length === 0) return null;
  return (
    <section className="mb-10">
      <h2 className="mb-3 text-sm font-semibold">{title}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(104px,1fr))] gap-x-3 gap-y-4">
        {pubs.map((p) => (
          <PubCover key={p.dir} pub={p} meta={`${p.symbol} · ${p.year}`} className="w-full" />
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
      <div className="max-w-xl rounded-md border border-dashed border-line px-5 py-6 text-sm leading-normal">
        <h2 className="mb-1 font-semibold">{t("Nothing downloaded in this language")}</h2>
        <p className="mb-4 text-muted">{t("Import .jwpub files, browse the publications, or pick another language.")}</p>
        <div className="flex flex-wrap gap-2">
          <Button onClick={() => void importFiles()}>
            <FolderInput size={16} /> {t("Import")}
          </Button>
          <Button variant="outline" onClick={() => push({ name: "library" })}>
            <CloudDownload size={16} /> {t("Publications")}
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

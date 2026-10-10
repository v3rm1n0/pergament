import { AppBar, useApp } from "@/app";
import { chapterTarget } from "@/lib/api";
import { usePublication } from "./publication";
import { t } from "@/lib/i18n";

export function ChaptersView({ dir, book }: { dir: string; book: number }) {
  const { openTarget } = useApp();
  const detail = usePublication(dir);
  const info = detail?.books.find((b) => b.number === book);
  if (!info) return <AppBar title="" />;
  return (
    <>
      <AppBar title={info.title} subtitle={detail!.card.shortTitle ?? detail!.card.title} />
      <div className="page flex-1 overflow-y-auto">
        <h2 className="mb-3 text-sm font-semibold">{t("Chapters")}</h2>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(3rem,1fr))] gap-2">
          {Array.from({ length: info.chapters }, (_, i) => i + 1).map((ch) => (
            <button
              key={ch}
              onClick={() => openTarget(chapterTarget(dir, book, ch))}
              className="flex aspect-square items-center justify-center rounded-md border border-line text-sm tabular-nums hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
            >
              {ch}
            </button>
          ))}
        </div>
      </div>
    </>
  );
}

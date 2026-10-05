import { AppBar, useApp } from "@/app";
import { chapterTarget } from "@/lib/api";
import { bookShade } from "@/lib/bible";
import { cn } from "@/lib/utils";
import { usePublication } from "./publication";

const shadeClass = ["bg-tile-0", "bg-tile-1", "bg-tile-2"] as const;

export function ChaptersView({ dir, book }: { dir: string; book: number }) {
  const { openTarget } = useApp();
  const detail = usePublication(dir);
  const info = detail?.books.find((b) => b.number === book);
  if (!info) return <AppBar title="" />;
  return (
    <>
      <AppBar title={detail!.card.shortTitle ?? detail!.card.title} />
      <div className="flex-1 overflow-y-auto px-8 py-8">
        <h2 className="mb-5 text-[1.65rem] font-bold uppercase tracking-wide">{info.title}</h2>
        <div className="grid grid-cols-[repeat(auto-fill,minmax(4.5rem,1fr))] gap-[3px]">
          {Array.from({ length: info.chapters }, (_, i) => i + 1).map((ch) => (
            <button
              key={ch}
              onClick={() => openTarget(chapterTarget(dir, book, ch))}
              className={cn(
                "flex aspect-square items-center justify-center text-xl text-tile-fg hover:brightness-125",
                shadeClass[bookShade(book)],
              )}
            >
              {ch}
            </button>
          ))}
        </div>
      </div>
    </>
  );
}

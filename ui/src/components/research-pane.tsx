import { useEffect, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { ChevronLeft, ChevronRight, Newspaper } from "lucide-react";
import { api, type ResearchEntry } from "@/lib/api";
import { t } from "@/lib/i18n";

/** What the Research Guide lists for one verse: excerpts of other publications. */
export function ResearchPane({
  dir,
  book,
  chapter,
  verse,
  onBack,
  onFollow,
}: {
  dir: string;
  book: number;
  chapter: number;
  verse: number;
  onBack: () => void;
  onFollow: (href: string) => void;
}) {
  const [entries, setEntries] = useState<ResearchEntry[] | null>(null);
  const [bookTitle, setBookTitle] = useState("");

  useEffect(() => {
    let live = true;
    setEntries(null);
    api
      .researchGuide(dir, book, chapter, verse)
      .then((e) => live && setEntries(e))
      .catch(() => live && setEntries([]));
    api
      .publication(dir)
      .then((d) => live && setBookTitle(d.books.find((b) => b.number === book)?.chapter_title ?? ""))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [dir, book, chapter, verse]);

  const onClick = (e: ReactMouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href && !href.startsWith("#")) onFollow(href);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center border-b border-line">
        <button aria-label={t("Back")} onClick={onBack} className="flex h-11 w-11 items-center justify-center text-accent">
          <ChevronLeft size={20} />
        </button>
        <div className="flex-1 pr-11 text-center leading-tight">
          <div className="text-sm">{t("Research Guide")}</div>
          {bookTitle && (
            <div className="text-[0.7rem] text-fg/70">
              {bookTitle} {chapter}:{verse}
            </div>
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {entries?.map((e, i) => (
          <section key={i} className="mb-1">
            <button
              onClick={() => onFollow(e.href)}
              title={e.location}
              className="flex w-full items-center gap-3 bg-bar py-1.5 pl-1.5 pr-3 text-left hover:brightness-110"
            >
              <div className="flex h-11 w-11 shrink-0 items-center justify-center bg-tile">
                <Newspaper size={20} strokeWidth={1.3} />
              </div>
              <div className="min-w-0 flex-1 leading-tight">
                <div className="truncate font-semibold">{e.publication}</div>
                <div className="truncate text-[0.8rem] text-fg/75">{e.subject}</div>
              </div>
              <ChevronRight size={20} className="shrink-0" />
            </button>
            <div
              className="reader pane-reader px-5 py-3"
              onClick={onClick}
              dangerouslySetInnerHTML={{ __html: e.html }}
            />
          </section>
        ))}
        {entries?.length === 0 && <p className="px-5 py-6 text-sm text-muted">{t("Nothing found.")}</p>}
      </div>
    </div>
  );
}

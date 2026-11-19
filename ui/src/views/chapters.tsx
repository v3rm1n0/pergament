import { useEffect, useRef, useState } from "react";
import type { MouseEvent } from "react";
import { Play } from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, chapterTarget, documentTarget, type BookOverview, type Page } from "@/lib/api";
import { usePublication } from "./publication";
import { hydrateMedia } from "@/lib/media";
import { t } from "@/lib/i18n";
import { BookmarksButton } from "@/components/bookmarks";

export function ChaptersView({ dir, book }: { dir: string; book: number }) {
  const { openTarget, toast, playRecording } = useApp();
  const detail = usePublication(dir);
  const [overview, setOverview] = useState<BookOverview | null>(null);
  const [page, setPage] = useState<Page | null>(null);
  const info = detail?.books.find((b) => b.number === book);
  const overviewRef = useRef<HTMLElement>(null);

  useEffect(() => {
    let live = true;
    setOverview(null);
    setPage(null);
    api
      .bookOverview(dir, book)
      .then(async (o) => {
        if (!live) return;
        setOverview(o);
        if (o.document != null) {
          const p = await api.renderPage(documentTarget(dir, o.document));
          if (live) setPage(p);
        }
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [dir, book]);

  // The page is inserted as HTML; its jwmedia: pictures need their sources.
  useEffect(() => {
    if (page && overviewRef.current) hydrateMedia(overviewRef.current);
  }, [page]);

  /** Entries of the overview link to verses; they open the chapter. */
  const follow = async (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a[href]");
    if (!a) return;
    e.preventDefault();
    try {
      const action = await api.linkAction(dir, a.getAttribute("href")!);
      switch (action.kind) {
        case "open":
          openTarget(action.target);
          break;
        case "external":
          await api.openExternal(action.url);
          break;
        case "media":
          await playRecording(action.media);
          break;
        case "missing":
          toast(t("Not in your library: {link}", { link: action.url ?? a.getAttribute("href") ?? "" }));
          break;
        case "ignore":
          break;
      }
    } catch (err) {
      toast(t("Cannot follow link: {error}", { error: String(err) }));
    }
  };

  if (!info) return <AppBar title="" />;
  return (
    <>
      <AppBar title={info.title} subtitle={detail!.card.shortTitle ?? detail!.card.title}>
        <BookmarksButton dir={dir} title={detail!.card.shortTitle ?? detail!.card.title} />
      </AppBar>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:flex-row lg:overflow-visible">
        <div className="page shrink-0 lg:min-w-0 lg:flex-1 lg:overflow-y-auto">
          <h2 className="mb-3 text-sm font-semibold">{t("Chapters")}</h2>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(3.5rem,1fr))] gap-2">
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
          {overview?.video && (
            <button
              onClick={() => void playRecording(overview.video!)}
              className="mt-4 flex items-center gap-2 rounded-md border border-line px-3 py-2 text-sm hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
            >
              <Play size={16} strokeWidth={1.6} /> {t("Introduction video")}
            </button>
          )}
        </div>
        {page && (
          <aside
            aria-label={t("Study pane")}
            className="study-pane shrink-0 border-t border-line bg-surface px-5 py-4 lg:w-[min(40%,34rem)] lg:overflow-y-auto lg:border-l lg:border-t-0"
          >
            <article ref={overviewRef} className="reader overview" onClick={(e) => void follow(e)} dangerouslySetInnerHTML={{ __html: page.html }} />
          </aside>
        )}
      </div>
    </>
  );
}

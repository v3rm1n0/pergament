import { MediaImg } from "@/components/media-img";
import { useEffect, useMemo, useRef } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { BookOpen, ChevronLeft, ChevronRight, CloudDownload } from "lucide-react";
import { taskKey, useApp } from "@/app";
import { api, type CatalogEntry, type Page, type Target } from "@/lib/api";
import { citedBy, extractParagraphs, markCited, splitPage } from "@/lib/page";
import { hydrateMedia } from "@/lib/media";
import { cn } from "@/lib/utils";
import { EntryImage, mb } from "@/components/catalog";
import { Progress } from "@/components/ui/progress";
import { ParallelPane } from "@/components/parallel-pane";
import { ResearchPane } from "@/components/research-pane";
import type { BibleRange } from "@/lib/parallel";
import { t } from "@/lib/i18n";

/** A link opened in the side pane instead of the main view. */
export type PaneRef =
  | { kind: "page"; href: string; target: Target; page: Page; studyNote: boolean }
  | { kind: "bible"; href: string; range: BibleRange }
  | { kind: "research"; href: string; dir: string; book: number; chapter: number; verse: number }
  | { kind: "missing"; href: string; entry: CatalogEntry | null; url: string | null };

/**
 * Shows a referenced passage next to the text. Only the publication bar at
 * the top opens it in the main view.
 */
export function ReferencePane({
  pref,
  onBack,
  onFollow,
  onRetry,
}: {
  pref: PaneRef;
  onBack: () => void;
  /** A link clicked inside the pane. */
  onFollow: (href: string) => void;
  /** The missing publication was downloaded; resolve the link again. */
  onRetry: () => void;
}) {
  if (pref.kind === "bible") return <ParallelPane href={pref.href} range={pref.range} onBack={onBack} />;
  if (pref.kind === "research") {
    return (
      <ResearchPane
        dir={pref.dir}
        book={pref.book}
        chapter={pref.chapter}
        verse={pref.verse}
        onBack={onBack}
        onFollow={onFollow}
      />
    );
  }
  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center border-b border-line">
        <button aria-label={t("Back")} onClick={onBack} className="flex h-11 w-11 items-center justify-center text-accent">
          <ChevronLeft size={20} />
        </button>
        <span className="flex-1 pr-11 text-center text-sm">{t("Publication Reference")}</span>
      </div>
      {pref.kind === "page" ? (
        <ReferencePage pref={pref} onFollow={onFollow} />
      ) : (
        <MissingReference pref={pref} onRetry={onRetry} />
      )}
    </div>
  );
}

function ReferencePage({ pref, onFollow }: { pref: Extract<PaneRef, { kind: "page" }>; onFollow: (href: string) => void }) {
  const { openTarget, publications } = useApp();
  const ref = useRef<HTMLDivElement>(null);
  // A publication reference shows only the cited paragraphs, like verses do.
  const { body, onlyCited } = useMemo(() => {
    const full = splitPage(pref.page.html).body;
    const cited = citedBy(pref.href);
    const part = cited?.kind === "paragraphs" ? extractParagraphs(full, cited) : null;
    return { body: part ?? full, onlyCited: part !== null };
  }, [pref.page.html, pref.href]);
  const card = publications.find((p) => p.dir === pref.target.publication);
  const isChapter = "chapter" in pref.target.kind;

  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    hydrateMedia(root);
    const first =
      (onlyCited ? null : markCited(root, citedBy(pref.href))) ??
      (pref.page.fragment ? root.querySelector(`[id="${CSS.escape(pref.page.fragment)}"]`) : null);
    if (first) first.scrollIntoView({ block: "start" });
    else root.parentElement?.scrollTo({ top: 0 });
  }, [body, onlyCited, pref.href, pref.page.fragment]);

  const onClick = (e: ReactMouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("#")) {
      ref.current?.querySelector(`[id="${CSS.escape(href.slice(1))}"]`)?.scrollIntoView({ block: "start" });
    } else {
      onFollow(href);
    }
  };

  return (
    <>
      <button
        onClick={() => openTarget(pref.target, pref.studyNote)}
        title={t("Open in the reader")}
        className="flex shrink-0 items-center gap-3 bg-bar py-1.5 pl-1.5 pr-3 text-left hover:brightness-110"
      >
        {card?.cover ? (
          <MediaImg src={card.cover} alt="" className="h-11 w-11 object-cover" draggable={false} />
        ) : (
          <div className="flex h-11 w-11 items-center justify-center bg-tile">
            <BookOpen size={20} strokeWidth={1.3} />
          </div>
        )}
        <div className="min-w-0 flex-1 leading-tight">
          <div className="truncate font-semibold">{pref.page.title}</div>
          <div className="truncate text-[0.8rem] text-fg/75">{card?.shortTitle ?? card?.title}</div>
        </div>
        <ChevronRight size={20} className="shrink-0" />
      </button>
      <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
        <div
          ref={ref}
          onClick={onClick}
          className={cn("reader pane-reader", isChapter && "bible")}
          dangerouslySetInnerHTML={{ __html: body }}
        />
      </div>
    </>
  );
}

function MissingReference({ pref, onRetry }: { pref: Extract<PaneRef, { kind: "missing" }>; onRetry: () => void }) {
  const { download, downloads } = useApp();
  const entry = pref.entry;
  const progress = entry ? downloads[taskKey(entry)] : undefined;
  return (
    <div className="flex flex-col items-center gap-4 px-6 py-10 text-center">
      {entry && <EntryImage entry={entry} className="h-28 w-28" />}
      <div>
        {entry && <div className="font-semibold">{entry.item.issue_title || entry.item.title}</div>}
        <p className="mt-1 text-sm text-muted">{t("This publication is not in your library.")}</p>
      </div>
      {entry && !progress && (
        <button
          onClick={async () => {
            if (await download(entry)) onRetry();
          }}
          className="flex items-center gap-2 bg-brand px-4 py-2 text-sm text-brand-fg hover:brightness-110"
        >
          <CloudDownload size={17} /> {t("Download ({size})", { size: mb(entry.item.size) })}
        </button>
      )}
      {progress && (
        <div className="w-48">
          <Progress value={progress.total ? (progress.done / progress.total) * 100 : null} />
        </div>
      )}
      {!entry && pref.url && (
        <button className="text-sm text-link hover:underline" onClick={() => void api.openExternal(pref.url!)}>
          {t("Open on jw.org")}
        </button>
      )}
    </div>
  );
}

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { ChevronLeft, ChevronRight, PanelRightClose, PanelRightOpen } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import { api, chapterTarget, type ChapterStudy, type Page, type Target, type VerseStudy } from "@/lib/api";
import { splitPage, verseKeyFromId } from "@/lib/page";
import { cn } from "@/lib/utils";
import { usePublication } from "./publication";

function flash(el: Element | null, block: ScrollLogicalPosition = "center") {
  if (!el) return;
  el.scrollIntoView({ block, behavior: "smooth" });
  el.classList.add("selected");
  setTimeout(() => el.classList.remove("selected"), 1600);
}

/** `chapter:verse`, the key of a verse within the current book. */
const verseKey = (v: { chapter: number; verse: number }) => `${v.chapter}:${v.verse}`;

/** `v1-12-3-1` -> `12:3`. */
function verseKeyOfSpan(id: string): string | null {
  const full = verseKeyFromId(id);
  return full ? full.split(":").slice(1).join(":") : null;
}

/** Outline range like "1-9" or "7". */
function outlineRange(o: ChapterStudy["outline"][number]): string {
  if (o.end_verse == null || (o.begin_chapter === o.end_chapter && o.begin_verse === o.end_verse)) {
    return `${o.begin_verse}`;
  }
  return o.begin_chapter === o.end_chapter
    ? `${o.begin_verse}-${o.end_verse}`
    : `${o.begin_chapter}:${o.begin_verse}–${o.end_chapter}:${o.end_verse}`;
}

export function ReaderView({ target, note }: { target: Target; note?: boolean }) {
  const { openTarget, replace, toast } = useApp();
  const [page, setPage] = useState<Page | null>(null);
  const [study, setStudy] = useState<ChapterStudy | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [paneOpen, setPaneOpen] = useState(true);
  const articleRef = useRef<HTMLDivElement>(null);
  const paneRef = useRef<HTMLElement>(null);
  const detail = usePublication(target.publication);
  const chapter = "chapter" in target.kind ? target.kind.chapter : null;

  useEffect(() => {
    let live = true;
    setPage(null);
    setStudy(null);
    setSelected(null);
    api
      .renderPage(target)
      .then((p) => live && setPage(p))
      .catch((e) => toast(`Cannot show page: ${e}`));
    if ("chapter" in target.kind) {
      const { book, chapter: c } = target.kind.chapter;
      api
        .chapterStudy(target.publication, book, c)
        .then((s) => live && setStudy(s))
        .catch((e) => toast(`Cannot load study notes: ${e}`));
    }
    return () => {
      live = false;
    };
  }, [target, toast]);

  const split = useMemo(() => (page ? splitPage(page.html) : null), [page]);

  // Lookups from text markers to the verse that owns them.
  const owners = useMemo(() => {
    const fn = new Map<number, string>();
    const xr = new Map<number, string>();
    const noted = new Set<string>();
    for (const v of study?.verses ?? []) {
      v.footnotes.forEach((f) => fn.set(f.index, verseKey(v)));
      v.xrefs.forEach((x) => xr.set(x.block, verseKey(v)));
      if (v.notes.length > 0) noted.add(verseKey(v));
    }
    return { fn, xr, noted };
  }, [study]);

  const selectVerse = useCallback((key: string) => {
    setSelected(key);
    setPaneOpen(true);
    requestAnimationFrame(() =>
      flash(paneRef.current?.querySelector(`[data-verse="${CSS.escape(key)}"]`) ?? null, "start"),
    );
  }, []);

  // Mark verses with study notes and jump to the requested verse.
  useEffect(() => {
    const root = articleRef.current;
    if (!root || !split) return;
    root.querySelectorAll<HTMLElement>("span.v[id]").forEach((el) => {
      const key = verseKeyOfSpan(el.id);
      if (key && owners.noted.has(key)) el.classList.add("has-notes");
    });
    if (page?.fragment) flash(root.querySelector(`[id="${CSS.escape(page.fragment)}"]`));
    else root.closest(".overflow-y-auto")?.scrollTo({ top: 0 });
    if (note && chapter && study) selectVerse(`${chapter.chapter}:${chapter.verse}`);
    // Runs once per loaded page and study data, not on every selection.
  }, [split, page, owners, study]);

  const follow = useCallback(
    async (href: string) => {
      try {
        const action = await api.linkAction(target.publication, href);
        switch (action.kind) {
          case "open": {
            const t = action.target;
            const k = t.kind;
            if (
              chapter &&
              "chapter" in k &&
              t.publication === target.publication &&
              k.chapter.book === chapter.book &&
              k.chapter.chapter === chapter.chapter
            ) {
              const { book, chapter: c, verse } = k.chapter;
              flash(articleRef.current?.querySelector(`[id="v${book}-${c}-${verse}-1"]`) ?? null);
              if (action.studyNote) selectVerse(`${c}:${verse}`);
            } else {
              openTarget(t, action.studyNote);
            }
            break;
          }
          case "external":
            await api.openExternal(action.url);
            break;
          case "missing":
            toast(
              action.url ? "This publication is not in your library" : "No Bible in your library",
              action.url ? { label: "Open on jw.org", run: () => void api.openExternal(action.url!) } : undefined,
            );
            break;
          case "ignore":
            break;
        }
      } catch (e) {
        toast(`Cannot follow link: ${e}`);
      }
    },
    [chapter, openTarget, selectVerse, target.publication, toast],
  );

  const onArticleClick = (e: ReactMouseEvent) => {
    const el = e.target as HTMLElement;
    const a = el.closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href") ?? "";
      const fn = /^#footnote(\d+)$/.exec(href);
      const xr = /^#xref(\d+)$/.exec(href);
      if (fn && owners.fn.has(Number(fn[1]))) selectVerse(owners.fn.get(Number(fn[1]))!);
      else if (xr && owners.xr.has(Number(xr[1]))) selectVerse(owners.xr.get(Number(xr[1]))!);
      else if (fn && split?.footnotes[`footnote${fn[1]}`]) {
        setSelected(`footnote${fn[1]}`);
        setPaneOpen(true);
      } else if (href.startsWith("#")) {
        flash(articleRef.current?.querySelector(`[id="${CSS.escape(href.slice(1))}"]`) ?? null);
      } else {
        void follow(href);
      }
      return;
    }
    const verse = el.closest(".vl, .cl")?.closest<HTMLElement>("span.v[id]");
    const key = verse ? verseKeyOfSpan(verse.id) : null;
    if (key && study?.verses.some((v) => verseKey(v) === key)) selectVerse(key);
  };

  const onPaneClick = (e: ReactMouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("#")) {
      flash(articleRef.current?.querySelector(`[id="${CSS.escape(href.slice(1))}"]`) ?? null);
    } else {
      void follow(href);
    }
  };

  const books = detail?.books;
  const go = (delta: number) => {
    if (!chapter || !books) return;
    const book = books.find((b) => b.number === chapter.book);
    if (!book) return;
    let [b, c] = [chapter.book, chapter.chapter + delta];
    if (c < 1) {
      const prev = books.find((x) => x.number === b - 1);
      if (!prev) return;
      [b, c] = [prev.number, prev.chapters];
    } else if (c > book.chapters) {
      if (!books.some((x) => x.number === b + 1)) return;
      [b, c] = [b + 1, 1];
    }
    replace({ name: "reader", target: chapterTarget(target.publication, b, c) });
  };

  useEffect(() => {
    if (!chapter) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement) return;
      if (e.key === "ArrowLeft" && !e.altKey) go(-1);
      if (e.key === "ArrowRight" && !e.altKey) go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const docFootnotes = !chapter && split ? Object.keys(split.footnotes) : [];
  const hasPane = chapter ? study !== null : docFootnotes.length > 0;
  const subtitle = detail?.card.title ?? "";
  const edge =
    "absolute top-1/2 z-10 flex h-10 w-7 -translate-y-1/2 items-center justify-center bg-bar/80 hover:bg-bar";

  return (
    <>
      <AppBar title={page?.title ?? ""} subtitle={subtitle}>
        {hasPane && (
          <BarButton label={paneOpen ? "Hide study pane" : "Show study pane"} onClick={() => setPaneOpen((o) => !o)}>
            {paneOpen ? (
              <PanelRightClose size={21} strokeWidth={1.5} />
            ) : (
              <PanelRightOpen size={21} strokeWidth={1.5} />
            )}
          </BarButton>
        )}
      </AppBar>
      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1">
          {chapter && (
            <>
              <button aria-label="Previous chapter" className={cn(edge, "left-1")} onClick={() => go(-1)}>
                <ChevronLeft size={18} />
              </button>
              <button aria-label="Next chapter" className={cn(edge, "right-1")} onClick={() => go(1)}>
                <ChevronRight size={18} />
              </button>
            </>
          )}
          <div className="h-full overflow-y-auto bg-surface">
            <div className={cn("mx-auto px-12 py-6", chapter ? "max-w-[46rem]" : "max-w-3xl")}>
              {split && (
                <div
                  ref={articleRef}
                  className={cn("reader", chapter && "bible")}
                  onClick={onArticleClick}
                  dangerouslySetInnerHTML={{ __html: split.body }}
                />
              )}
            </div>
          </div>
        </div>
        {hasPane && paneOpen && (
          <aside
            ref={paneRef}
            onClick={onPaneClick}
            className="study-pane w-[min(44%,640px)] shrink-0 overflow-y-auto border-l border-line bg-surface px-5 py-4 text-[0.93rem] leading-relaxed"
          >
            {chapter && study ? (
              <StudyPane book={chapter.book} study={study} selected={selected} />
            ) : (
              docFootnotes.map((id) => (
                <div
                  key={id}
                  className={cn("pane-content mb-3", selected === id && "selected")}
                  dangerouslySetInnerHTML={{ __html: split!.footnotes[id] }}
                />
              ))
            )}
          </aside>
        )}
      </div>
    </>
  );
}

function VerseSection({ v, selected }: { v: VerseStudy; selected: boolean }) {
  return (
    <section data-verse={verseKey(v)} className={cn("mb-5 scroll-mt-2 px-1", selected && "selected")}>
      <h3 className="mb-2 text-[1.05rem] font-semibold">{verseKey(v)}</h3>
      {v.footnotes.map((f) => (
        <div key={`f${f.index}`} className="mb-2 flex gap-2">
          <span className="text-link">*</span>
          <div className="pane-content" dangerouslySetInnerHTML={{ __html: f.html }} />
        </div>
      ))}
      {v.xrefs.map((x) => (
        <p key={`x${x.block}`} className="mb-2">
          <span className="mr-2 italic text-fg/80">{x.marker}</span>
          {x.refs.map((r, i) => (
            <span key={i}>
              <a href={r.href}>{r.label}</a>
              {i < x.refs.length - 1 && "; "}
            </span>
          ))}
        </p>
      ))}
      {v.notes.map((n, i) => (
        <div key={`n${i}`} className="pane-content mb-2" dangerouslySetInnerHTML={{ __html: n }} />
      ))}
    </section>
  );
}

function StudyPane({ book, study, selected }: { book: number; study: ChapterStudy; selected: string | null }) {
  return (
    <>
      {study.outline.length > 0 && (
        <section className="mb-5">
          <h3 className="mb-2 text-[1.05rem] font-semibold">{study.outlineTitle ?? "Outline"}</h3>
          {study.outline.map((o, i) => (
            <div key={i} style={{ paddingLeft: `${Math.max(0, o.level - 2) * 1.3}rem` }}>
              {o.text}{" "}
              <a className="text-xs" href={`#v${book}-${o.begin_chapter}-${o.begin_verse}-1`}>
                ({outlineRange(o)})
              </a>
            </div>
          ))}
        </section>
      )}
      {study.verses.map((v) => (
        <VerseSection key={verseKey(v)} v={v} selected={selected === verseKey(v)} />
      ))}
    </>
  );
}

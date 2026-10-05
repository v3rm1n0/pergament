import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { ChevronLeft, ChevronRight, PanelRightClose, PanelRightOpen } from "lucide-react";
import { AppBar, useApp } from "@/app";
import { api, chapterTarget, type Page, type Target } from "@/lib/api";
import { splitPage, verseKeyFromId } from "@/lib/page";
import { Button } from "@/components/ui/button";
import { usePublication } from "./publication";

type Selection =
  | { kind: "footnote"; id: string }
  | { kind: "xref"; id: string }
  | { kind: "notes"; verse: string }
  | null;

function flash(el: Element | null) {
  if (!el) return;
  el.scrollIntoView({ block: "center", behavior: "smooth" });
  el.classList.add("selected");
  setTimeout(() => el.classList.remove("selected"), 1600);
}

export function ReaderView({ target, note }: { target: Target; note?: boolean }) {
  const { openTarget, replace, toast } = useApp();
  const [page, setPage] = useState<Page | null>(null);
  const [selection, setSelection] = useState<Selection>(null);
  const [paneOpen, setPaneOpen] = useState(true);
  const articleRef = useRef<HTMLDivElement>(null);
  const detail = usePublication(target.publication);
  const chapter = "chapter" in target.kind ? target.kind.chapter : null;

  useEffect(() => {
    let live = true;
    setPage(null);
    setSelection(null);
    api
      .renderPage(target)
      .then((p) => live && setPage(p))
      .catch((e) => toast(`Cannot show page: ${e}`));
    return () => {
      live = false;
    };
  }, [target, toast]);

  const split = useMemo(() => (page ? splitPage(page.html) : null), [page]);
  const hasPane =
    split !== null &&
    (Object.keys(split.footnotes).length > 0 || Object.keys(split.xrefs).length > 0 || split.noteOrder.length > 0);

  // Mark verses with study notes and jump to the requested verse.
  useEffect(() => {
    const root = articleRef.current;
    if (!root || !split) return;
    root.querySelectorAll<HTMLElement>("span.v[id]").forEach((el) => {
      const key = verseKeyFromId(el.id);
      if (key && split.notes[key]) el.classList.add("has-notes");
    });
    if (page?.fragment) flash(root.querySelector(`[id="${CSS.escape(page.fragment)}"]`));
    else root.parentElement?.scrollTo({ top: 0 });
    // Arrived via a study-note link: show that note.
    if (note && chapter) {
      const key = `${chapter.book}:${chapter.chapter}:${chapter.verse}`;
      if (split.notes[key]) setSelection({ kind: "notes", verse: key });
    }
  }, [split, page]);

  const follow = useCallback(
    async (href: string) => {
      try {
        const action = await api.linkAction(target.publication, href);
        switch (action.kind) {
          case "open": {
            const t = action.target;
            const k = t.kind;
            // Same chapter: just scroll to the verse.
            if (
              chapter &&
              "chapter" in k &&
              t.publication === target.publication &&
              k.chapter.book === chapter.book &&
              k.chapter.chapter === chapter.chapter
            ) {
              const { book, chapter: c, verse } = k.chapter;
              flash(articleRef.current?.querySelector(`[id="v${book}-${c}-${verse}-1"]`) ?? null);
              const key = `${book}:${c}:${verse}`;
              if (action.studyNote && split?.notes[key]) setSelection({ kind: "notes", verse: key });
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
    [chapter, openTarget, split, target.publication, toast],
  );

  const onArticleClick = (e: ReactMouseEvent) => {
    const el = e.target as HTMLElement;
    const a = el.closest("a");
    if (a) {
      e.preventDefault();
      const href = a.getAttribute("href") ?? "";
      if (/^#footnote\d+$/.test(href)) {
        setSelection({ kind: "footnote", id: href.slice(1) });
        setPaneOpen(true);
      } else if (/^#xref\d+$/.test(href)) {
        setSelection({ kind: "xref", id: href.slice(1) });
        setPaneOpen(true);
      } else if (href.startsWith("#")) {
        flash(articleRef.current?.querySelector(`[id="${CSS.escape(href.slice(1))}"]`) ?? null);
      } else {
        void follow(href);
      }
      return;
    }
    const num = el.closest(".vl, .cl");
    const verse = num?.closest<HTMLElement>("span.v[id]");
    const key = verse ? verseKeyFromId(verse.id) : null;
    if (key && split?.notes[key]) {
      setSelection({ kind: "notes", verse: key });
      setPaneOpen(true);
    }
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

  return (
    <>
      <AppBar title={page?.title ?? ""}>
        {hasPane && (
          <Button
            variant="ghost"
            size="icon"
            title={paneOpen ? "Hide study pane" : "Show study pane"}
            aria-label="Toggle study pane"
            onClick={() => setPaneOpen((o) => !o)}
          >
            {paneOpen ? <PanelRightClose size={22} strokeWidth={1.6} /> : <PanelRightOpen size={22} strokeWidth={1.6} />}
          </Button>
        )}
      </AppBar>
      <div className="flex min-h-0 flex-1">
        <div className="flex-1 overflow-y-auto bg-surface">
          <div className="mx-auto max-w-3xl px-10 py-8">
            {split && (
              <div
                ref={articleRef}
                className="reader"
                onClick={onArticleClick}
                dangerouslySetInnerHTML={{ __html: split.body }}
              />
            )}
            {chapter && (
              <div className="mt-10 flex justify-between border-t border-line pt-5">
                <Button variant="outline" onClick={() => go(-1)}>
                  <ChevronLeft size={18} /> Previous
                </Button>
                <Button variant="outline" onClick={() => go(1)}>
                  Next <ChevronRight size={18} />
                </Button>
              </div>
            )}
          </div>
        </div>
        {hasPane && paneOpen && split && (
          <StudyPane split={split} selection={selection} onSelect={setSelection} onClick={onPaneClick} />
        )}
      </div>
    </>
  );
}

function StudyPane({
  split,
  selection,
  onSelect,
  onClick,
}: {
  split: NonNullable<ReturnType<typeof splitPage>>;
  selection: Selection;
  onSelect: (s: Selection) => void;
  onClick: (e: ReactMouseEvent) => void;
}) {
  const selectedHtml =
    selection?.kind === "footnote"
      ? split.footnotes[selection.id]
      : selection?.kind === "xref"
        ? split.xrefs[selection.id]
        : selection?.kind === "notes"
          ? split.notes[selection.verse]?.join("")
          : undefined;
  const heading =
    selection?.kind === "footnote" ? "Footnote" : selection?.kind === "xref" ? "Cross references" : "Study notes";

  return (
    <aside className="flex w-[380px] shrink-0 flex-col border-l border-line bg-bg" onClick={onClick}>
      <div className="border-b border-line px-5 py-3 text-xs font-semibold uppercase tracking-wider text-muted">
        {selectedHtml ? heading : "Study"}
      </div>
      <div className="flex-1 overflow-y-auto px-5 py-4">
        {selectedHtml ? (
          <div className="pane-content" dangerouslySetInnerHTML={{ __html: selectedHtml }} />
        ) : split.noteOrder.length > 0 ? (
          <div className="space-y-5">
            {split.noteOrder.map((key) => (
              <div
                key={key}
                className="pane-content cursor-pointer rounded bg-surface p-3 ring-1 ring-line hover:ring-accent"
                onClick={(e) => {
                  if (!(e.target as HTMLElement).closest("a")) onSelect({ kind: "notes", verse: key });
                }}
                dangerouslySetInnerHTML={{ __html: split.notes[key].join("") }}
              />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted">Select a footnote or cross-reference marker in the text.</p>
        )}
      </div>
      {selectedHtml && (
        <button
          className="border-t border-line px-5 py-3 text-left text-sm text-accent hover:bg-bar"
          onClick={(e) => {
            e.stopPropagation();
            onSelect(null);
          }}
        >
          {split.noteOrder.length > 0 ? "Show all study notes" : "Close"}
        </button>
      )}
    </aside>
  );
}

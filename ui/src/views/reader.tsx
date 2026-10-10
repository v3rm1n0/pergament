import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { ChevronLeft, ChevronRight, Newspaper, PanelRightClose, PanelRightOpen } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import {
  api,
  chapterTarget,
  documentOrder,
  documentTarget,
  type ChapterStudy,
  type Mark,
  type MarkRange,
  type Note,
  type NoteInput,
  type Page,
  type PageUserData,
  type Target,
  type VerseStudy,
} from "@/lib/api";
import { applyMarks, blockElements, markText, rangeText, selectionRanges, titleFromText } from "@/lib/marks";
import { loadAnswer, saveAnswer, saveLastColor } from "@/lib/settings";
import { clickedImage, hydrateMedia } from "@/lib/media";
import { parseBibleRange } from "@/lib/parallel";
import { MarkToolbar, NoteCard, NoteEditor } from "@/components/notes";
import { splitPage, verseKeyFromId } from "@/lib/page";
import { cn } from "@/lib/utils";
import { addDays, fromDateNumber, isoDate, longDate } from "@/lib/dates";
import { ReferencePane, type PaneRef } from "@/components/reference-pane";
import { usePublication } from "./publication";
import { t } from "@/lib/i18n";

/** How a followed link changes the pane: new stack, deeper, or retry in place. */
type RefMode = "root" | "push" | "replace";

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
  const { push, replace, toast, lang, userVersion, publications, playRecording, showImage, layout } = useApp();
  const [refs, setRefs] = useState<PaneRef[]>([]);
  const [page, setPage] = useState<Page | null>(null);
  const [study, setStudy] = useState<ChapterStudy | null>(null);
  /** Verses of the chapter the Research Guide has excerpts for. */
  const [research, setResearch] = useState<number[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [paneOpen, setPaneOpen] = useState(true);
  const articleRef = useRef<HTMLDivElement>(null);
  /** Answers typed on this page, which win over the loaded ones until they are reloaded. */
  const answers = useRef<Record<string, string>>({});
  const paneRef = useRef<HTMLElement>(null);
  const detail = usePublication(target.publication);
  const chapter = "chapter" in target.kind ? target.kind.chapter : null;
  const dated = "dated" in target.kind ? target.kind.dated.date : null;

  useEffect(() => {
    let live = true;
    setPage(null);
    setStudy(null);
    setResearch([]);
    setSelected(null);
    answers.current = {};
    setRefs([]);
    api
      .renderPage(target)
      .then((p) => live && setPage(p))
      .catch((e) => toast(t("Cannot show page: {error}", { error: String(e) })));
    if ("chapter" in target.kind) {
      const { book, chapter: c } = target.kind.chapter;
      api
        .chapterStudy(target.publication, book, c)
        .then((s) => live && setStudy(s))
        .catch((e) => toast(t("Cannot load study notes: {error}", { error: String(e) })));
      api
        .researchVerses(target.publication, book, c)
        .then((v) => live && setResearch(v))
        .catch(() => undefined);
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

  // Highlights and notes of this page from user data.
  const [user, setUser] = useState<PageUserData>({ marks: [], notes: [], answers: {} });
  const reloadUser = useCallback(() => {
    api
      .pageUserData(target)
      .then(setUser)
      .catch(() => setUser({ marks: [], notes: [], answers: {} }));
    // userVersion: reload after a backup was restored.
  }, [target, userVersion]);
  useEffect(reloadUser, [reloadUser]);

  // Redraw the text with verse markers, highlights and note markers.
  useEffect(() => {
    const root = articleRef.current;
    if (!root || !split) return;
    root.innerHTML = split.body;
    root.querySelectorAll<HTMLElement>("span.v[id]").forEach((el) => {
      const key = verseKeyOfSpan(el.id);
      if (key && owners.noted.has(key)) el.classList.add("has-notes");
    });
    // Answer fields of workbooks and study articles become text boxes. Their
    // text is kept in the user data under the id of the original text area.
    root.querySelectorAll<HTMLElement>("div.gen-field[id]").forEach((el) => {
      const tag = el.querySelector(".gen-tag")?.id;
      const legacy = `${JSON.stringify(target)}#${el.id}`;
      const box = document.createElement("textarea");
      box.className = "gen-field-input";
      box.rows = 3;
      box.setAttribute("aria-label", el.textContent?.trim() ?? "");
      box.value = tag ? (answers.current[tag] ?? user.answers[tag] ?? "") : loadAnswer(legacy);
      let timer: number | undefined;
      const save = () => {
        window.clearTimeout(timer);
        if (!tag) return saveAnswer(legacy, box.value);
        answers.current[tag] = box.value;
        api.saveAnswer(target, tag, box.value).catch((e) => toast(String(e)));
      };
      box.addEventListener("input", () => {
        window.clearTimeout(timer);
        if (!tag) return saveAnswer(legacy, box.value);
        answers.current[tag] = box.value;
        timer = window.setTimeout(save, 600);
      });
      box.addEventListener("blur", () => timer !== undefined && save());
      el.replaceWith(box);
    });
    hydrateMedia(root);
    applyMarks(root, user.marks);
    for (const n of user.notes) {
      if (n.blockIdentifier == null) continue;
      const first = blockElements(root, n.blockType, n.blockIdentifier)[0];
      if (!first) continue;
      const dot = first.querySelector<HTMLElement>(":scope > .note-dot") ?? document.createElement("span");
      dot.className = "note-dot";
      dot.dataset.notes = [...(dot.dataset.notes?.split(" ") ?? []), n.guid].join(" ");
      dot.setAttribute("role", "button");
      dot.setAttribute("aria-label", t("Show note"));
      first.prepend(dot);
    }
  }, [split, owners, user, target]);

  // Jump to the requested verse.
  useEffect(() => {
    const root = articleRef.current;
    if (!root || !split) return;
    if (page?.fragment) flash(root.querySelector(`[id="${CSS.escape(page.fragment)}"]`));
    else root.closest(".overflow-y-auto")?.scrollTo({ top: 0 });
    if (note && chapter && study) selectVerse(`${chapter.chapter}:${chapter.verse}`);
    // Runs once per loaded page and study data, not on every selection.
  }, [split, page, owners, study]);

  /** Show a reference in the pane: replace the pane stack, push onto it, or replace its top. */
  const showRef = useCallback((ref: PaneRef, mode: RefMode) => {
    setRefs((r) => (mode === "root" ? [ref] : mode === "push" ? [...r, ref] : [...r.slice(0, -1), ref]));
    setPaneOpen(true);
  }, []);

  // Links open in the pane; only the pane's publication bar opens them in the reader.
  const follow = useCallback(
    async (href: string, mode: RefMode = "root") => {
      try {
        const action = await api.linkAction(target.publication, href);
        switch (action.kind) {
          case "open": {
            const t = action.target;
            const k = t.kind;
            if (
              mode === "root" &&
              chapter &&
              "chapter" in k &&
              t.publication === target.publication &&
              k.chapter.book === chapter.book &&
              k.chapter.chapter === chapter.chapter
            ) {
              const { book, chapter: c, verse } = k.chapter;
              flash(articleRef.current?.querySelector(`[id="v${book}-${c}-${verse}-1"]`) ?? null);
              if (action.studyNote) selectVerse(`${c}:${verse}`);
            } else if (parseBibleRange(href) && publications.some((p) => p.isBible)) {
              showRef({ kind: "bible", href, range: parseBibleRange(href)! }, mode);
            } else {
              const p = await api.renderPage(t);
              showRef({ kind: "page", href, target: t, page: p, studyNote: action.studyNote }, mode);
            }
            break;
          }
          case "external":
            await api.openExternal(action.url);
            break;
          case "media":
            await playRecording(action.media);
            break;
          case "missing": {
            const entry = await api.missingEntry(lang, href).catch(() => null);
            showRef({ kind: "missing", href, entry, url: action.url }, mode);
            break;
          }
          case "ignore":
            break;
        }
      } catch (e) {
        toast(t("Cannot follow link: {error}", { error: String(e) }));
      }
    },
    [chapter, lang, playRecording, publications, selectVerse, showRef, target.publication, toast],
  );

  // Highlighting: a selection or a click on a highlight shows the toolbar.
  const [toolbar, setToolbar] = useState<Toolbar | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const closeToolbar = useCallback(() => setToolbar(null), []);

  const onArticleMouseUp = () => {
    const root = articleRef.current;
    const sel = window.getSelection();
    if (!root || !sel || sel.isCollapsed || !root.contains(sel.anchorNode)) return;
    const range = sel.getRangeAt(0);
    const ranges = selectionRanges(root, range);
    if (ranges.length === 0) return;
    const rect = range.getBoundingClientRect();
    setToolbar({ x: rect.left + rect.width / 2, y: rect.top - 6, ranges, text: rangeText(range) });
  };

  const userAction = async (run: () => Promise<unknown>) => {
    try {
      await run();
    } catch (e) {
      toast(String(e));
    }
    window.getSelection()?.removeAllRanges();
    setToolbar(null);
    reloadUser();
  };

  const colorChosen = (color: number) => {
    const t = toolbar;
    if (!t) return;
    saveLastColor(color);
    void userAction(() => (t.mark ? api.setMarkColor(t.mark.guid, color) : api.addMark(target, color, t.ranges ?? [])));
  };

  /** Note on the toolbar's highlight or, for a plain selection, on its text without highlighting it. */
  const noteChosen = async () => {
    const t = toolbar;
    if (!t) return;
    setToolbar(null);
    try {
      const markGuid = t.mark?.guid;
      const block = (t.mark?.ranges ?? t.ranges ?? [])[0];
      const existing = markGuid ? user.notes.find((n) => n.markGuid === markGuid) : undefined;
      const marked = t.mark && articleRef.current ? markText(articleRef.current, t.mark.guid) : (t.text ?? "");
      window.getSelection()?.removeAllRanges();
      reloadUser();
      setEditing(
        existing
          ? { note: existing }
          : { note: { title: titleFromText(marked) }, markGuid, blockType: block?.blockType, blockIdentifier: block?.identifier },
      );
    } catch (e) {
      toast(String(e));
    }
  };

  const saveNote = (input: Omit<NoteInput, "guid" | "blockType" | "blockIdentifier" | "markGuid">) => {
    const ed = editing;
    if (!ed) return;
    setEditing(null);
    void userAction(() =>
      api.saveNote(ed.note.guid ? null : target, {
        ...input,
        guid: ed.note.guid,
        blockType: ed.blockType ?? 0,
        blockIdentifier: ed.blockIdentifier ?? null,
        markGuid: ed.markGuid ?? null,
      }),
    );
  };

  /** Open the study pane on the notes and point at one. */
  const showNote = (guid?: string) => {
    setRefs([]);
    setPaneOpen(true);
    requestAnimationFrame(() => flash(paneRef.current?.querySelector(`[data-note="${guid}"]`) ?? null, "start"));
  };

  const onArticleClick = (e: ReactMouseEvent) => {
    if (!window.getSelection()?.isCollapsed) return;
    const el = e.target as HTMLElement;
    const picture = clickedImage(el);
    if (picture) {
      showImage(picture.src, picture.caption);
      return;
    }
    const dot = el.closest<HTMLElement>(".note-dot");
    if (dot) {
      showNote(dot.dataset.notes?.split(" ")[0]);
      return;
    }
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
    const hl = el.closest<HTMLElement>("mark.hl");
    const mark = hl && user.marks.find((m) => m.guid === hl.dataset.guid);
    if (mark) {
      const rect = hl.getBoundingClientRect();
      setToolbar({ x: rect.left + rect.width / 2, y: rect.top - 6, mark });
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
  const docs = useMemo(() => (detail ? documentOrder(detail.toc) : []), [detail]);
  const docId = "document" in target.kind ? target.kind.document : null;
  const docIndex = docId !== null ? docs.indexOf(docId) : -1;
  const paged = chapter !== null || dated !== null || docIndex >= 0;
  const go = (delta: number) => {
    if (dated !== null) {
      // The next day may be in another year's booklet.
      const day = addDays(fromDateNumber(dated), delta);
      api
        .datedPage(lang, "dailyText", isoDate(day))
        .then((p) =>
          p ? replace({ name: "reader", target: p.target }) : toast(t("No daily text for {date} in your library", { date: longDate(day) })),
        )
        .catch((e) => toast(String(e)));
      return;
    }
    if (docIndex >= 0) {
      const id = docs[docIndex + delta];
      if (id !== undefined) replace({ name: "reader", target: documentTarget(target.publication, id) });
      return;
    }
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
    if (!paged) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (editing) return;
      if (e.key === "ArrowLeft" && !e.altKey) go(-1);
      if (e.key === "ArrowRight" && !e.altKey) go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const docFootnotes = !chapter && split ? Object.keys(split.footnotes) : [];
  const hasPane =
    (chapter ? study !== null : docFootnotes.length > 0) || refs.length > 0 || user.notes.length > 0;
  const subtitle = detail?.card.title ?? "";
  const edge =
    "absolute top-1/2 z-10 flex h-10 w-7 -translate-y-1/2 items-center justify-center rounded-md border border-line bg-surface text-muted hover:bg-bar hover:text-fg";

  return (
    <>
      <AppBar title={dated !== null ? t("Daily Text") : (page?.title ?? "")} subtitle={subtitle}>
        {hasPane && (
          <BarButton label={paneOpen ? "Hide study pane" : "Show study pane"} onClick={() => setPaneOpen((o) => !o)}>
            {paneOpen ? (
              <PanelRightClose size={18} strokeWidth={1.6} />
            ) : (
              <PanelRightOpen size={18} strokeWidth={1.6} />
            )}
          </BarButton>
        )}
      </AppBar>
      <div className="flex min-h-0 flex-1">
        <div className="relative min-w-0 flex-1">
          {paged && (
            <>
              <button
                aria-label={dated !== null ? t("Previous day") : chapter ? t("Previous chapter") : t("Previous page")}
                className={cn(edge, "left-1")}
                onClick={() => go(-1)}
              >
                <ChevronLeft size={18} />
              </button>
              <button
                aria-label={dated !== null ? t("Next day") : chapter ? t("Next chapter") : t("Next page")}
                className={cn(edge, "right-1")}
                onClick={() => go(1)}
              >
                <ChevronRight size={18} />
              </button>
            </>
          )}
          <div className="h-full overflow-y-auto bg-surface">
            <div className={cn("mx-auto px-12 py-6", layout === "centered" && (chapter ? "max-w-[46rem]" : "max-w-3xl"))}>
              {split && (
                <div
                  ref={articleRef}
                  className={cn("reader", chapter && "bible")}
                  onClick={onArticleClick}
                  onMouseUp={onArticleMouseUp}
                  dangerouslySetInnerHTML={{ __html: split.body }}
                />
              )}
              {split && dated !== null && detail && (
                <button
                  className="mt-6 text-[1.4rem] text-link hover:underline"
                  onClick={() => push({ name: "publication", dir: target.publication })}
                >
                  {detail.card.title}
                </button>
              )}
            </div>
          </div>
        </div>
        {hasPane && paneOpen && refs.length > 0 && (
          <aside className="study-pane w-[min(44%,640px)] shrink-0 border-l border-line bg-surface text-[0.93rem] leading-relaxed">
            <ReferencePane
              pref={refs[refs.length - 1]}
              onBack={() => setRefs((r) => r.slice(0, -1))}
              onFollow={(href) => void follow(href, "push")}
              onRetry={() => void follow(refs[refs.length - 1].href, "replace")}
            />
          </aside>
        )}
        {hasPane && paneOpen && refs.length === 0 && (
          <aside
            ref={paneRef}
            onClick={onPaneClick}
            className="study-pane w-[min(44%,640px)] shrink-0 overflow-y-auto border-l border-line bg-surface px-5 py-4 text-[0.93rem] leading-relaxed"
          >
            {user.notes.length > 0 && (
              <section className="mb-6 flex flex-col gap-2">
                <h3 className="text-sm font-semibold">{t("My notes")}</h3>
                {user.notes.map((n) => (
                  <div key={n.guid} data-note={n.guid}>
                    <NoteCard note={n} onClick={() => setEditing({ note: n })} />
                  </div>
                ))}
              </section>
            )}
            {chapter && study ? (
              <StudyPane
                book={chapter.book}
                chapter={chapter.chapter}
                study={study}
                selected={selected}
                research={research}
                onResearch={(verse) =>
                  showRef(
                    {
                      kind: "research",
                      href: `research:${target.publication}:${chapter.book}:${chapter.chapter}:${verse}`,
                      dir: target.publication,
                      book: chapter.book,
                      chapter: chapter.chapter,
                      verse,
                    },
                    "root",
                  )
                }
              />
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
      {toolbar && (
        <MarkToolbar
          x={toolbar.x}
          y={toolbar.y}
          current={toolbar.mark?.color}
          onColor={colorChosen}
          onNote={() => void noteChosen()}
          onDelete={toolbar.mark ? () => void userAction(() => api.deleteMark(toolbar.mark!.guid)) : undefined}
          onClose={closeToolbar}
        />
      )}
      {editing && (
        <NoteEditor
          note={editing.note}
          onSave={saveNote}
          onDelete={
            editing.note.guid
              ? () => {
                  const guid = editing.note.guid!;
                  setEditing(null);
                  void userAction(() => api.deleteNote(guid));
                }
              : undefined
          }
          onClose={() => setEditing(null)}
        />
      )}
    </>
  );
}

/** Where the highlight toolbar is and what it acts on: a new selection or a highlight. */
interface Toolbar {
  x: number;
  y: number;
  ranges?: MarkRange[];
  mark?: Mark;
  /** The selected text, for a new selection. */
  text?: string;
}

/** The note being written, and where a new one attaches. */
interface Editing {
  note: Partial<Note>;
  markGuid?: string;
  blockType?: number;
  blockIdentifier?: number;
}

function VerseSection({
  v,
  selected,
  research,
  onResearch,
}: {
  v: VerseStudy;
  selected: boolean;
  research: boolean;
  onResearch: () => void;
}) {
  return (
    <section data-verse={verseKey(v)} className={cn("mb-5 scroll-mt-2 px-1", selected && "selected")}>
      <h3 className="mb-2 text-sm font-semibold">{verseKey(v)}</h3>
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
      {research && (
        <button onClick={onResearch} className="mb-2 flex items-center gap-2 text-link hover:underline">
          <Newspaper size={17} strokeWidth={1.5} /> {t("Research Guide")}
        </button>
      )}
    </section>
  );
}

function StudyPane({
  book,
  chapter,
  study,
  selected,
  research,
  onResearch,
}: {
  book: number;
  chapter: number;
  study: ChapterStudy;
  selected: string | null;
  research: number[];
  onResearch: (verse: number) => void;
}) {
  // Verses that only have Research Guide excerpts get a section of their own.
  const verses = [...study.verses];
  for (const verse of research) {
    if (!verses.some((v) => v.verse === verse)) verses.push({ chapter, verse, footnotes: [], xrefs: [], notes: [] });
  }
  verses.sort((a, b) => a.chapter - b.chapter || a.verse - b.verse);
  return (
    <>
      {study.outline.length > 0 && (
        <section className="mb-5">
          <h3 className="mb-2 text-sm font-semibold">{study.outlineTitle ?? t("Outline")}</h3>
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
      {verses.map((v) => (
        <VerseSection
          key={verseKey(v)}
          v={v}
          selected={selected === verseKey(v)}
          research={research.includes(v.verse)}
          onResearch={() => onResearch(v.verse)}
        />
      ))}
    </>
  );
}

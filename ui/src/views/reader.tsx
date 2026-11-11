import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import { ChevronLeft, ChevronRight, Newspaper, PanelRightClose, PanelRightOpen, X } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import {
  api,
  chapterTarget,
  documentOrder,
  documentTarget,
  type ChapterStudy,
  type Mark,
  type MarkRange,
  type Page,
  type PageUserData,
  type Target,
  type VerseStudy,
} from "@/lib/api";
import { restoreScroll } from "@/lib/scroll";
import { applyMarks, blockElements, markText, rangeText, selectionRanges } from "@/lib/marks";
import { loadAnswer, saveAnswer, saveLastColor } from "@/lib/settings";
import { clickedImage, hydrateMedia } from "@/lib/media";
import { parseBibleRange } from "@/lib/parallel";
import { InlineNoteEditor, MarkToolbar, NoteCard } from "@/components/notes";
import { GoToButton } from "@/components/go-to";
import { OutlineSection } from "@/components/outline-section";
import { splitPage, verseKeyFromId } from "@/lib/page";
import { cn } from "@/lib/utils";
import { isCompact, useCompact } from "@/lib/viewport";
import { addDays, fromDateNumber, isoDate, longDate } from "@/lib/dates";
import { ReferencePane, type PaneRef } from "@/components/reference-pane";
import { usePublication } from "./publication";
import { t } from "@/lib/i18n";

/** How a followed link changes the pane: new stack, deeper, or retry in place. */
type RefMode = "root" | "push" | "replace";

function flash(el: Element | null, block: ScrollLogicalPosition = "center", behavior: ScrollBehavior = "smooth") {
  if (!el) return;
  el.scrollIntoView({ block, behavior });
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

export function ReaderView({
  target,
  note,
  noteGuid,
  scroll,
}: {
  target: Target;
  note?: boolean;
  /** A note to jump to once the text is shown. */
  noteGuid?: string;
  scroll?: number;
}) {
  const {
    push,
    replace,
    toast,
    lang,
    userVersion,
    notesVersion,
    publications,
    playRecording,
    showImage,
    viewer,
    layout,
    note: written,
    releaseNote,
    keepNote,
    openNote,
    setLinkHandler,
  } = useApp();
  /** The note being written in the pane; one moving to the note window is gone from here already. */
  const inlineNote = written?.moving ? null : written;
  const [refs, setRefs] = useState<PaneRef[]>([]);
  const [page, setPage] = useState<Page | null>(null);
  const [study, setStudy] = useState<ChapterStudy | null>(null);
  /** Verses of the chapter the Research Guide has excerpts for. */
  const [research, setResearch] = useState<number[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  // A phone shows the pane as a sheet over the text, so it waits for a tap; a desktop column is open from the start.
  const [paneOpen, setPaneOpen] = useState(() => !isCompact());
  // On a phone the study pane is a sheet over the text, not a column beside it.
  const compact = useCompact();
  const articleRef = useRef<HTMLDivElement>(null);
  /** Answers typed on this page, which win over the loaded ones until they are reloaded. */
  const answers = useRef<Record<string, string>>({});
  const paneRef = useRef<HTMLElement>(null);
  const resumed = useRef(false);
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
  /** Notes in the study pane opened to their whole text. */
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const toggleNote = (guid: string) =>
    setExpanded((s) => {
      const next = new Set(s);
      if (!next.delete(guid)) next.add(guid);
      return next;
    });
  const reloadUser = useCallback(() => {
    api
      .pageUserData(target)
      .then(setUser)
      .catch(() => setUser({ marks: [], notes: [], answers: {} }));
    // userVersion: reload after a backup was restored; notesVersion: after the note editor saved.
  }, [target, userVersion, notesVersion]);
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
    // Answer fields of workbooks and study articles: text boxes, checkboxes and lines for a date. Each is kept in
    // the user data under the id of the original field (`TextTag`), as JW Library does; a checkbox as "1" or "0".
    const keep = (tag: string, value: string, wait: number) => {
      answers.current[tag] = value;
      window.clearTimeout(timers.get(tag));
      timers.set(
        tag,
        window.setTimeout(() => api.saveAnswer(target, tag, value).catch((e) => toast(String(e))), wait),
      );
    };
    const timers = new Map<string, number>();
    root.querySelectorAll<HTMLElement>("span.gen-tag[id]").forEach((marker) => {
      const tag = marker.id;
      const value = answers.current[tag] ?? user.answers[tag] ?? "";
      const label = root.querySelector<HTMLElement>(`label[for="${CSS.escape(tag)}"]`)?.textContent?.trim();
      const name = label || marker.closest(".gen-field")?.textContent?.trim() || "";
      let field: HTMLInputElement | HTMLTextAreaElement;
      if (marker.classList.contains("gen-check")) {
        const box = document.createElement("input");
        box.type = "checkbox";
        box.className = "gen-field-check";
        box.checked = value === "1";
        box.addEventListener("change", () => keep(tag, box.checked ? "1" : "0", 0));
        field = box;
      } else if (marker.classList.contains("gen-line")) {
        const line = document.createElement("input");
        line.type = "text";
        line.className = "gen-field-line";
        line.value = value;
        line.addEventListener("input", () => keep(tag, line.value, 600));
        field = line;
      } else {
        const box = document.createElement("textarea");
        box.className = "gen-field-input";
        box.rows = 3;
        box.value = value;
        box.addEventListener("input", () => keep(tag, box.value, 600));
        field = box;
      }
      field.id = tag;
      if (name) field.setAttribute("aria-label", name);
      // A pending save goes out when the field is left.
      field.addEventListener("blur", () => {
        const pending = timers.get(tag);
        if (pending === undefined) return;
        window.clearTimeout(pending);
        timers.delete(tag);
        api.saveAnswer(target, tag, answers.current[tag] ?? "").catch((e) => toast(String(e)));
      });
      marker.replaceWith(field);
    });
    // Older publications have no field inside: the whole block becomes one text box, kept on this computer.
    root.querySelectorAll<HTMLElement>("div.gen-field[id]").forEach((el) => {
      if (el.querySelector("input, textarea")) return;
      const legacy = `${JSON.stringify(target)}#${el.id}`;
      const box = document.createElement("textarea");
      box.className = "gen-field-input";
      box.rows = 3;
      box.setAttribute("aria-label", el.textContent?.trim() ?? "");
      box.value = loadAnswer(legacy);
      box.addEventListener("input", () => saveAnswer(legacy, box.value));
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
      if (n.color && !dot.dataset.color) dot.dataset.color = String(n.color);
      dot.dataset.notes = [...(dot.dataset.notes?.split(" ") ?? []), n.guid].join(" ");
      dot.setAttribute("role", "button");
      dot.setAttribute("aria-label", t("Show note"));
      first.prepend(dot);
    }
  }, [split, owners, user, target]);

  // Jump to the note that was opened from Personal Study, once.
  const jumped = useRef(false);
  useEffect(() => {
    const root = articleRef.current;
    const n = noteGuid ? user.notes.find((x) => x.guid === noteGuid) : undefined;
    if (!root || !split || !n || n.blockIdentifier == null || jumped.current || scroll) return;
    const block = blockElements(root, n.blockType, n.blockIdentifier)[0];
    if (!block) return;
    jumped.current = true;
    flash(block, "center", "instant");
  }, [split, user, noteGuid, scroll]);

  // Jump to the requested verse.
  useEffect(() => {
    const root = articleRef.current;
    if (!root || !split) return;
    if (page?.fragment) flash(root.querySelector(`[id="${CSS.escape(page.fragment)}"]`));
    else {
      const el = root.closest<HTMLElement>(".overflow-y-auto");
      // Coming back to the tab continues where the user stopped, once.
      if (scroll && el && !resumed.current) restoreScroll(el, scroll);
      else if (!(noteGuid && user.notes.some((n) => n.guid === noteGuid && n.blockIdentifier != null))) el?.scrollTo({ top: 0 });
      resumed.current = true;
    }
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
          case "extract":
            showRef(
              {
                kind: "extract",
                href,
                url: action.url,
                publication: action.publication,
                subject: action.subject,
                location: action.location,
                html: action.html,
              },
              mode,
            );
            break;
          case "ignore":
            break;
        }
      } catch (e) {
        toast(t("Cannot follow link: {error}", { error: String(e) }));
      }
    },
    [chapter, lang, playRecording, publications, selectVerse, showRef, target.publication, toast],
  );

  /** The bar of an embedded excerpt was clicked; offer to download the quoted publication. */
  const openSource = useCallback(
    async (href: string, url: string | null) => {
      const entry = await api.missingEntry(lang, href).catch(() => null);
      showRef({ kind: "missing", href, entry, url }, "push");
    },
    [lang, showRef],
  );

  // Highlighting: a selection or a click on a highlight shows the toolbar.
  const [toolbar, setToolbar] = useState<Toolbar | null>(null);
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
      setRefs([]);
      setPaneOpen(true);
      await openNote(
        existing
          ? { note: existing }
          : { note: { title: marked }, target, markGuid, blockType: block?.blockType, blockIdentifier: block?.identifier },
        { in: "pane" },
      );
    } catch (e) {
      toast(String(e));
    }
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

  /** A note's links to the app's own pages open like a cross-reference, also those clicked in the note window. */
  const followNoteLink = (href: string) => void follow(href);
  const latestFollow = useRef(follow);
  latestFollow.current = follow;
  useEffect(() => {
    setLinkHandler((href) => void latestFollow.current(href));
    return () => setLinkHandler(null);
  }, [setLinkHandler]);

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
      // Keys pressed in the note editor belong to it.
      if (e.target instanceof Element && e.target.closest("[data-note-editor]")) return;
      // A picture or the full-size player over the page has the keys (the player marks the ones it took).
      if (viewer || e.defaultPrevented) return;
      if (e.key === "ArrowLeft" && !e.altKey) go(-1);
      if (e.key === "ArrowRight" && !e.altKey) go(1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const docFootnotes = !chapter && split ? Object.keys(split.footnotes) : [];
  /** The inline draft is new, or its note is not among this page's notes: it shows above them. */
  const detached = inlineNote !== null && (!inlineNote.draft.note.guid || !user.notes.some((n) => n.guid === inlineNote.draft.note.guid));
  const hasPane =
    (chapter ? study !== null : docFootnotes.length > 0) || refs.length > 0 || user.notes.length > 0 || inlineNote !== null;
  // The note being written lives in the pane: when the page changes or the pane closes,
  // unsaved text moves to the note window and an unchanged note closes. Behind a followed
  // reference it waits, and comes back with its text when the reference closes.
  useEffect(() => releaseNote, [target, releaseNote]);
  useEffect(() => {
    if (!paneOpen) releaseNote();
    else if (refs.length > 0) keepNote();
  }, [paneOpen, refs.length, releaseNote, keepNote]);
  const subtitle = detail?.card.title ?? "";
  const edge =
    "absolute top-1/2 z-10 flex h-10 w-7 -translate-y-1/2 items-center justify-center rounded-md border border-line bg-surface text-muted hover:bg-bar hover:text-fg";

  return (
    <>
      <AppBar title={dated !== null ? t("Daily Text") : (page?.title ?? "")} subtitle={subtitle}>
        {chapter && <GoToButton />}
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
            <div
              className={cn(
                "mx-auto py-6",
                compact ? "px-4" : "px-12",
                layout === "centered" && (chapter ? "max-w-[46rem]" : "max-w-3xl"),
              )}
            >
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
          <aside
            aria-label={t("Study pane")}
            className={cn(
              "study-pane bg-surface text-[0.93rem] leading-relaxed",
              compact
                ? "fixed inset-0 z-30 flex flex-col pt-[env(safe-area-inset-top)] pb-[env(safe-area-inset-bottom)]"
                : "w-[min(44%,640px)] shrink-0 border-l border-line",
            )}
          >
            {compact && <SheetBar onClose={() => setPaneOpen(false)} />}
            <div className="flex min-h-0 flex-1 flex-col">
              <ReferencePane
                pref={refs[refs.length - 1]}
                onBack={() => setRefs((r) => r.slice(0, -1))}
                onFollow={(href) => void follow(href, "push")}
                onOpenSource={(href, url) => void openSource(href, url)}
                onRetry={() => void follow(refs[refs.length - 1].href, "replace")}
              />
            </div>
          </aside>
        )}
        {hasPane && paneOpen && refs.length === 0 && (
          <aside
            aria-label={t("Study pane")}
            ref={paneRef}
            onClick={onPaneClick}
            className={cn(
              "study-pane overflow-y-auto bg-surface text-[0.93rem] leading-relaxed",
              compact
                ? "fixed inset-0 z-30 px-5 pt-1 pb-[calc(1rem+env(safe-area-inset-bottom))]"
                : "w-[min(44%,640px)] shrink-0 border-l border-line px-5 py-4",
            )}
          >
            {compact && <SheetBar onClose={() => setPaneOpen(false)} sticky />}
            {(user.notes.length > 0 || detached) && (
              <section className="mb-6 flex flex-col gap-2">
                <h3 className="text-sm font-semibold">{t("My notes")}</h3>
                {detached && <InlineNoteEditor onAppLink={followNoteLink} />}
                {user.notes.map((n) => (
                  <div key={n.guid} data-note={n.guid}>
                    {inlineNote?.draft.note.guid === n.guid ? (
                      <InlineNoteEditor onAppLink={followNoteLink} />
                    ) : (
                      <NoteCard
                        note={n}
                        expanded={expanded.has(n.guid)}
                        onToggle={() => toggleNote(n.guid)}
                        onClick={() => void openNote({ note: n }, { in: "pane" })}
                        onAppLink={followNoteLink}
                      />
                    )}
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
    </>
  );
}

/** On a phone the study pane covers the text, so it needs its own way back. `sticky` keeps it in view while the pane scrolls. */
function SheetBar({ onClose, sticky }: { onClose: () => void; sticky?: boolean }) {
  return (
    <div className={cn("flex h-11 items-center justify-end", sticky && "sticky top-0 z-10 -mx-5 bg-surface px-5")}>
      <button
        aria-label={t("Close")}
        onClick={onClose}
        className="flex size-11 items-center justify-center text-muted hover:text-fg"
      >
        <X size={20} strokeWidth={1.6} />
      </button>
    </div>
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

function RefLinks({ refs }: { refs: VerseStudy["xrefs"][number]["refs"] }) {
  return refs.map((r, i) => (
    <span key={i}>
      <a href={r.href}>{r.label}</a>
      {i < refs.length - 1 && "; "}
    </span>
  ));
}

/** A marginal reference block; the parallel accounts of the verse are set apart from the other references. */
function XrefBlock({ x }: { x: VerseStudy["xrefs"][number] }) {
  const parallel = x.refs.filter((r) => r.parallel);
  const others = x.refs.filter((r) => !r.parallel);
  return (
    <>
      {parallel.length > 0 && (
        <p className="mb-2">
          <span className="mr-2 italic text-fg/80">{x.marker}</span>
          <span className="mr-1 font-semibold">{t("Parallel accounts")}:</span>
          <RefLinks refs={parallel} />
        </p>
      )}
      {others.length > 0 && (
        <p className="mb-2">
          {parallel.length === 0 && <span className="mr-2 italic text-fg/80">{x.marker}</span>}
          <RefLinks refs={others} />
        </p>
      )}
    </>
  );
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
        <XrefBlock key={`x${x.block}`} x={x} />
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
      <OutlineSection title={study.outlineTitle} outline={study.outline} book={book} />
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

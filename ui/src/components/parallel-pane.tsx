import { useEffect, useMemo, useRef, useState } from "react";
import { BookOpen, ChevronDown, ChevronLeft, ChevronRight, ChevronUp, CircleMinus, CirclePlus } from "lucide-react";
import { useApp } from "@/app";
import { api, chapterTarget, type PubCard } from "@/lib/api";
import { t } from "@/lib/i18n";
import { extractVerses, rangeText, type BibleRange } from "@/lib/parallel";
import { loadParallel, saveParallel } from "@/lib/settings";
import { MediaImg } from "@/components/media-img";

/** Verses of a Bible link in every included Bible of the library. */
export function ParallelPane({ href, range, onBack }: { href: string; range: BibleRange; onBack: () => void }) {
  const { publications, lang, languageName, openTarget } = useApp();
  const bibles = useMemo(() => publications.filter((p) => p.isBible), [publications]);
  const [included, setIncluded] = useState<string[] | null>(loadParallel);
  const [customizing, setCustomizing] = useState(false);
  const [texts, setTexts] = useState<Record<string, string>>({});
  const [bookTitle, setBookTitle] = useState("");
  const loaded = useRef(new Map<string, string>());

  const shown = useMemo(() => {
    if (included) {
      return included.flatMap((d) => bibles.find((b) => b.dir === d) ?? []);
    }
    // Bibles in the selected language first.
    return [...bibles].sort((a, b) => Number(b.langCode === lang) - Number(a.langCode === lang));
  }, [bibles, included, lang]);

  useEffect(() => {
    let live = true;
    for (const bible of shown) {
      const key = `${bible.dir}|${href}`;
      const cached = loaded.current.get(key);
      if (cached !== undefined) {
        setTexts((s) => (s[bible.dir] === cached ? s : { ...s, [bible.dir]: cached }));
        continue;
      }
      const chapters = Array.from({ length: range.to[0] - range.from[0] + 1 }, (_, i) => range.from[0] + i);
      Promise.all(chapters.map((c) => api.renderPage(chapterTarget(bible.dir, range.book, c))))
        .then((pages) => extractVerses(pages.map((p) => p.html).join(""), range))
        .catch(() => "")
        .then((html) => {
          loaded.current.set(key, html);
          if (live) setTexts((s) => ({ ...s, [bible.dir]: html }));
        });
    }
    return () => {
      live = false;
    };
  }, [shown, href, range]);

  // Book name in the first Bible that has the book.
  useEffect(() => {
    let live = true;
    const first = shown[0];
    if (!first) return;
    api
      .publication(first.dir)
      .then((d) => live && setBookTitle(d.books.find((b) => b.number === range.book)?.title ?? ""))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [shown, range.book]);

  const save = (dirs: string[]) => {
    saveParallel(dirs);
    setIncluded(dirs);
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-11 shrink-0 items-center border-b border-line">
        <button aria-label={t("Back")} onClick={onBack} className="flex h-11 w-11 items-center justify-center text-accent">
          <ChevronLeft size={20} />
        </button>
        <div className="flex-1 pr-11 text-center leading-tight">
          <div className="text-sm">{t("Parallel Translations")}</div>
          {bookTitle && <div className="text-[0.7rem] text-fg/70">{rangeText(bookTitle, range)}</div>}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {shown.map((b) => (
          <section key={b.dir} className="mb-1">
            <button
              onClick={() => openTarget(chapterTarget(b.dir, range.book, range.from[0], range.from[1]))}
              title={t("Open in the reader")}
              className="flex w-full items-center gap-3 bg-bar py-1.5 pl-1.5 pr-3 text-left hover:brightness-110"
            >
              <Cover pub={b} />
              <div className="min-w-0 flex-1 leading-tight">
                <div className="truncate font-semibold">{b.shortTitle ?? b.title}</div>
                <div className="truncate text-[0.8rem] text-fg/75">{b.langCode ? languageName(b.langCode) : ""}</div>
              </div>
              <ChevronRight size={20} className="shrink-0" />
            </button>
            <div
              className="reader pane-reader bible px-5 py-3"
              dangerouslySetInnerHTML={{ __html: texts[b.dir] ?? "" }}
            />
          </section>
        ))}
        <div className="flex justify-center px-5 py-6">
          <button onClick={() => setCustomizing(true)} className="w-72 bg-tile px-4 py-2 text-sm hover:brightness-110">
            {t("Customize")}
          </button>
        </div>
      </div>
      {customizing && (
        <CustomizeDialog
          bibles={bibles}
          included={shown.map((b) => b.dir)}
          onChange={save}
          onClose={() => setCustomizing(false)}
        />
      )}
    </div>
  );
}

function Cover({ pub }: { pub: PubCard }) {
  return pub.cover ? (
    <MediaImg src={pub.cover} alt="" className="h-11 w-11 object-cover" draggable={false} />
  ) : (
    <div className="flex h-11 w-11 items-center justify-center bg-tile">
      <BookOpen size={20} strokeWidth={1.3} />
    </div>
  );
}

function CustomizeDialog({
  bibles,
  included,
  onChange,
  onClose,
}: {
  bibles: PubCard[];
  included: string[];
  onChange: (dirs: string[]) => void;
  onClose: () => void;
}) {
  const { languageName } = useApp();
  const move = (i: number, by: number) => {
    const next = [...included];
    [next[i], next[i + by]] = [next[i + by], next[i]];
    onChange(next);
  };
  const row = (b: PubCard, i: number | null) => (
    <div key={b.dir} className="flex items-center gap-1">
      <button
        onClick={() => onChange(i === null ? [...included, b.dir] : included.filter((d) => d !== b.dir))}
        className="flex min-w-0 flex-1 items-center gap-4 py-2 text-left"
      >
        {i === null ? (
          <CirclePlus className="shrink-0 text-green-500" size={24} />
        ) : (
          <CircleMinus className="shrink-0 text-red-500" size={24} />
        )}
        <span className="min-w-0 leading-tight">
          <span className="block truncate text-xs text-muted">{b.langCode ? languageName(b.langCode) : ""}</span>
          <span className="block truncate text-sm">{b.shortTitle ?? b.title}</span>
        </span>
      </button>
      {i !== null && (
        <>
          <button
            aria-label={t("Move up")}
            title={t("Move up")}
            disabled={i === 0}
            onClick={() => move(i, -1)}
            className="p-1.5 hover:text-accent disabled:opacity-30"
          >
            <ChevronUp size={20} />
          </button>
          <button
            aria-label={t("Move down")}
            title={t("Move down")}
            disabled={i === included.length - 1}
            onClick={() => move(i, 1)}
            className="p-1.5 hover:text-accent disabled:opacity-30"
          >
            <ChevronDown size={20} />
          </button>
        </>
      )}
    </div>
  );
  const out = bibles.filter((b) => !included.includes(b.dir));
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" onMouseDown={onClose}>
      <div
        onMouseDown={(e) => e.stopPropagation()}
        className="flex w-[min(90vw,420px)] flex-col gap-3 bg-surface p-5 shadow-2xl ring-1 ring-line"
      >
        <p className="text-center text-sm text-muted">{t("Import or download more Bibles to add translations.")}</p>
        <div>{included.flatMap((d) => bibles.find((b) => b.dir === d) ?? []).map((b, i) => row(b, i))}</div>
        {out.length > 0 && (
          <div>
            <h3 className="mb-1 text-sm font-semibold">{t("Not Included")}</h3>
            {out.map((b) => row(b, null))}
          </div>
        )}
        <div className="flex justify-end">
          <button onClick={onClose} className="bg-tile px-6 py-2 text-sm hover:brightness-110">
            {t("Done")}
          </button>
        </div>
      </div>
    </div>
  );
}

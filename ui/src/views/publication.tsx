import { useEffect, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { DownloadCloud, FileText, Star } from "lucide-react";
import { AppBar, BarButton, useApp } from "@/app";
import { api, documentTarget, samePublication, type LanguageEntry, type PubCard, type PubDetail, type TocNode } from "@/lib/api";
import { cardTarget } from "@/components/tile-menu";
import { booksTabIndex, hasBooks, shortBookName } from "@/lib/bible";
import { t } from "@/lib/i18n";
import { confirmText, failedText, importedText } from "@/lib/references";
import { GoToButton } from "@/components/go-to";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** The last loaded detail per publication. The view remounts on every tab switch, so this keeps it from being blank until the reload finishes. */
const cachedDetails = new Map<string, PubDetail>();

export function usePublication(dir: string): PubDetail | null {
  const { toast } = useApp();
  const [detail, setDetail] = useState<PubDetail | null>(cachedDetails.get(dir) ?? null);
  useEffect(() => {
    let live = true;
    // Another publication must not show the previous one's tabs and books.
    setDetail(cachedDetails.get(dir) ?? null);
    api
      .publication(dir)
      .then((d) => {
        cachedDetails.set(dir, d);
        if (live) setDetail(d);
      })
      .catch((e) => toast(t("Cannot open publication: {error}", { error: String(e) })));
    return () => {
      live = false;
    };
  }, [dir, toast]);
  return detail;
}

/** One Testament: heading plus its books as a grid that is wide enough for the full short names. */
function BookSection({ dir, node }: { dir: string; node: TocNode }) {
  const { push } = useApp();
  const books = node.children.filter((c) => c.bible_book != null);
  return (
    <section>
      <h2 className="mb-3 text-sm font-semibold">{node.title}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(136px,1fr))] gap-2">
        {books.map((b) => (
          <button
            key={b.bible_book}
            onClick={() => push({ name: "chapters", dir, book: b.bible_book! })}
            className="flex h-10 items-center rounded-md border border-line px-3 text-left text-sm hover:bg-bar focus-visible:outline-2 focus-visible:outline-accent"
          >
            <span className="truncate">{shortBookName(b.title)}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

function BooksTab({ dir, node }: { dir: string; node: TocNode }) {
  // The books tab holds one child per Testament; they stack, so the books keep their room.
  return (
    <div className="flex flex-col gap-10">
      {node.children.filter(hasBooks).map((s) => (
        <BookSection key={s.title} dir={dir} node={s} />
      ))}
    </div>
  );
}

/** Documents of a TOC subtree as a tile list; nested groups get headings. */
function DocList({ dir, nodes }: { dir: string; nodes: TocNode[] }) {
  const { openTarget } = useApp();
  const docs = nodes.filter((n) => n.document_id != null && n.children.length === 0);
  const groups = nodes.filter((n) => n.children.length > 0);
  return (
    <div className="space-y-8">
      {docs.length > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-2">
          {docs.map((d, i) => (
            <button
              key={`${d.document_id}-${i}`}
              onClick={() => openTarget(documentTarget(dir, d.document_id!))}
              className="flex min-h-12 items-center gap-3 rounded-md border border-line bg-surface px-3 py-2 text-left text-sm hover:bg-bar"
            >
              <FileText className="shrink-0 text-muted" size={18} strokeWidth={1.5} />
              <span className="line-clamp-2">{d.title}</span>
            </button>
          ))}
        </div>
      )}
      {groups.map((g, i) => (
        <section key={`${g.title}-${i}`}>
          <h2 className="mb-3 text-sm font-semibold">{g.title}</h2>
          <DocList dir={dir} nodes={g.children} />
        </section>
      ))}
    </div>
  );
}

/** Star in the top bar that adds the publication to the Home favorites. */
function FavoriteButton({ card }: { card: PubCard }) {
  const { favorites, setFavorite, lang } = useApp();
  const key = cardTarget(card, lang);
  const on = favorites.some((k) => samePublication(k, key));
  return (
    <BarButton label={on ? "Remove from favorites" : "Add to favorites"} onClick={() => void setFavorite(key, !on)}>
      <Star size={18} strokeWidth={1.6} className={on ? "fill-accent text-accent" : undefined} />
    </BarButton>
  );
}

/** Downloads the publications this one links to, after showing what that costs. */
function ReferencesButton({ dir, lang }: { dir: string; lang: string }) {
  const { download, toast, catalogVersion } = useApp();
  // `undefined` while scanning, `null` without a catalog.
  const [missing, setMissing] = useState<LanguageEntry[] | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const running = useRef(false);
  useEffect(() => {
    let live = true;
    api.referencedPublications(dir, lang).then((m) => live && setMissing(m), () => undefined);
    return () => {
      live = false;
    };
  }, [dir, lang, catalogVersion]);
  if (missing === undefined || (missing !== null && missing.length === 0)) return null;
  const run = async () => {
    if (missing === null) {
      toast(t("Load the catalog first to download referenced publications."));
      return;
    }
    if (running.current) return;
    running.current = true;
    setBusy(true);
    try {
      const ok = await ask(confirmText(missing.map((m) => m.entry)), {
        title: t("Download referenced publications?"),
        okLabel: t("Download"),
      });
      if (!ok) return;
      let imported = 0;
      let failed = 0;
      for (const { code, entry } of missing) {
        if ((await download(entry, code, { quiet: true })) === null) failed++;
        else imported++;
      }
      if (imported > 0) toast(importedText(imported));
      if (failed > 0) toast(failedText(failed));
    } finally {
      running.current = false;
      setBusy(false);
    }
  };
  return (
    <BarButton label="Download referenced publications (needs network)" onClick={() => void run()} disabled={busy}>
      <DownloadCloud size={18} strokeWidth={1.6} />
    </BarButton>
  );
}

export function PublicationView({ dir, tab }: { dir: string; tab?: number }) {
  const { replace } = useApp();
  const { lang, languageName } = useApp();
  const detail = usePublication(dir);
  if (!detail) return <AppBar title="" />;
  const title = detail.card.shortTitle ?? detail.card.title;
  const toc = detail.toc;
  const publicationLang = detail.card.langCode ?? lang;

  // A single root (e.g. a magazine issue) has no tabs; list its documents.
  if (toc.length <= 1) {
    return (
      <>
        <AppBar title={title} subtitle={languageName(publicationLang)}>
          {detail.card.isBible && <GoToButton />}
          <ReferencesButton dir={dir} lang={publicationLang} />
          <FavoriteButton card={detail.card} />
        </AppBar>
        <div className="page flex-1 overflow-y-auto">
          <DocList dir={dir} nodes={toc[0]?.children ?? []} />
        </div>
      </>
    );
  }

  const active = String(tab ?? booksTabIndex(toc));
  return (
    <>
      <AppBar title={title} subtitle={languageName(publicationLang)}>
        {detail.card.isBible && <GoToButton />}
        <ReferencesButton dir={dir} lang={publicationLang} />
        <FavoriteButton card={detail.card} />
      </AppBar>
      <Tabs
        value={active}
        onValueChange={(v) => replace({ name: "publication", dir, tab: Number(v) })}
        className="flex min-h-0 flex-1 flex-col"
      >
        <div className="page pb-0 pt-6">
          <TabsList>
            {toc.map((t, i) => (
              <TabsTrigger key={i} value={String(i)}>
                {t.title}
              </TabsTrigger>
            ))}
          </TabsList>
        </div>
        {toc.map((t, i) => (
          <TabsContent key={i} value={String(i)} className="page flex-1 pt-6 overflow-y-auto outline-none">
            {hasBooks(t) ? <BooksTab dir={dir} node={t} /> : <DocList dir={dir} nodes={t.children} />}
          </TabsContent>
        ))}
      </Tabs>
    </>
  );
}
